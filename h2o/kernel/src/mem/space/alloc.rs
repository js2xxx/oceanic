use alloc::collections::BTreeMap;
use core::{alloc::Layout, pin::Pin};

use bitop_ex::BitOpEx;
use canary::Canary;
use collection_ex::RangeSet;
use paging::{LAddr, PAddr};
use spin::Mutex;

use super::{AllocType, ArchSpace, Flags, SpaceError};

#[derive(Debug)]
pub struct Allocator {
    canary: Canary<Allocator>,
    free_range: Mutex<RangeSet<LAddr>>,
    record: Mutex<BTreeMap<LAddr, (Layout, Option<PAddr>)>>,
}

impl Allocator {
    pub const fn new(free_range: RangeSet<LAddr>) -> Self {
        Allocator {
            canary: Canary::new(),
            free_range: Mutex::new(free_range),
            record: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn alloc<'a, 'b>(
        &'a self,
        ty: AllocType,
        phys: Option<PAddr>,
        flags: Flags,
        arch: &'b ArchSpace,
    ) -> Result<Pin<&'a mut [u8]>, SpaceError> {
        self.canary.assert();

        if phys.map_or(false, |phys| phys.contains_bit(paging::PAGE_MASK)) {
            return Err(SpaceError::InvalidFormat);
        }

        // Get the virtual address.
        // `prefix` and `suffix` are the gaps beside the allocated address range.
        let mut range = self.free_range.lock();

        let (layout, size, prefix, virt, suffix) = match ty {
            AllocType::Layout(layout) => {
                // Calculate the real size used.
                let layout = layout.align_to(paging::PAGE_LAYOUT.align()).unwrap();
                let size = layout.pad_to_align().size();
                let (prefix, virt, suffix) = {
                    let res = range.range_iter().find_map(|r| {
                        let mut start = r.start.val();
                        while start & (layout.align() - 1) != 0 {
                            start += 1 << start.trailing_zeros();
                        }
                        if start + size <= r.end.val() {
                            Some((
                                r.start..LAddr::from(start),
                                LAddr::from(start)..LAddr::from(start + size),
                                LAddr::from(start + size)..r.end,
                            ))
                        } else {
                            None
                        }
                    });
                    res.ok_or(SpaceError::AddressBusy)?
                };
                (layout, size, prefix, virt, suffix)
            }
            AllocType::Virt(virt) => {
                let size = unsafe { virt.end.offset_from(*virt.start) } as usize;
                let layout = Layout::from_size_align(size, paging::PAGE_SIZE)
                    .map_err(|_| SpaceError::InvalidFormat)?;

                let (prefix, suffix) = {
                    let res = range.range_iter().find_map(|r| {
                        (r.start <= virt.start && virt.end <= r.end)
                            .then_some((r.start..virt.start, virt.end..r.end))
                    });

                    res.ok_or(SpaceError::AddressBusy)?
                };
                (layout, size, prefix, virt, suffix)
            }
        };

        // Get the physical address mapped to.
        let (phys, alloc_ptr) = match phys {
            Some(phys) => (phys, None),
            None => {
                let ptr = unsafe {
                    if flags.contains(Flags::ZEROED) {
                        alloc::alloc::alloc_zeroed(layout)
                    } else {
                        alloc::alloc::alloc(layout)
                    }
                };

                if ptr.is_null() {
                    return Err(SpaceError::OutOfMemory);
                }

                (LAddr::new(ptr).to_paddr(minfo::ID_OFFSET), Some(ptr))
            }
        };

        // Map it.
        let ptr = *virt.start;
        arch.maps(virt, phys, flags).map_err(|e| {
            if let Some(alloc_ptr) = alloc_ptr {
                unsafe { alloc::alloc::dealloc(alloc_ptr, layout) };
            }
            SpaceError::PagingError(e)
        })?;

        range.remove(prefix.start);
        if !prefix.is_empty() {
            let _ = range.insert(prefix.clone());
        }
        if !suffix.is_empty() {
            let _ = range.insert(suffix.clone());
        }
        drop(range);

        let ret = unsafe { Pin::new_unchecked(core::slice::from_raw_parts_mut(ptr, size)) };
        let _ = self
            .record
            .lock()
            .insert(LAddr::new(ptr), (layout, alloc_ptr.map(|_| phys)))
            .map(|_| panic!("Duplicate allocation"));

        Ok(ret)
    }

    pub unsafe fn modify<'a, 'b>(
        &'a self,
        mut b: Pin<&'a mut [u8]>,
        flags: Flags,
        arch: &'b ArchSpace,
    ) -> Result<Pin<&'a mut [u8]>, SpaceError> {
        self.canary.assert();

        let virt = {
            let ptr = b.as_mut_ptr_range();
            LAddr::new(ptr.start)..LAddr::new(ptr.end)
        };

        arch.reprotect(virt, flags)
            .map_err(SpaceError::PagingError)?;

        Ok(b)
    }

    pub unsafe fn dealloc<'a, 'b>(
        &'a self,
        mut b: Pin<&'a mut [u8]>,
        arch: &'b ArchSpace,
    ) -> Result<(), SpaceError> {
        self.canary.assert();

        let mut virt = {
            let ptr = b.as_mut_ptr_range();
            LAddr::new(ptr.start)..LAddr::new(ptr.end)
        };

        // Get the virtual address range from the given memory block.
        let layout = Layout::for_value(&*b)
            .align_to(paging::PAGE_SIZE)
            .map_err(|_| SpaceError::InvalidFormat)?
            .pad_to_align();
        let phys = {
            let mut record = self.record.lock();
            match record.remove(&virt.start) {
                Some((l, p)) if layout.size() != l.size() => {
                    record.insert(virt.start, (l, p));
                    return Err(SpaceError::InvalidFormat);
                }
                None => return Err(SpaceError::InvalidFormat),
                Some((_, p)) => p,
            }
        };

        // Unmap the virtual address & get the physical address.
        let _ = arch.unmaps(virt.clone()).map_err(SpaceError::PagingError)?;

        if let Some(phys) = phys {
            let alloc_ptr = phys.to_laddr(minfo::ID_OFFSET);
            alloc::alloc::dealloc(*alloc_ptr, layout);
        }

        // Deallocate the virtual address range.
        let mut range = self.free_range.lock();
        let (prefix, suffix) = range.neighbors(virt.clone());
        if let Some(prefix) = prefix {
            virt.start = prefix.start;
            range.remove(prefix.start);
        }
        if let Some(suffix) = suffix {
            virt.end = suffix.end;
            range.remove(suffix.start);
        }
        range.insert(virt).map_err(|_| SpaceError::AddressBusy)
    }

    pub unsafe fn dispose_mapping(&self, _arch: &ArchSpace) {
        // TODO: Be aware of shared page tables.

        // let record = self.record.lock();
        // for (&base, (layout, _)) in record.iter() {
        //       let virt = base..LAddr::from(base.val() +
        // layout.pad_to_align().size());       let _ =
        // arch.unmaps(virt); }
    }
}

impl Drop for Allocator {
    fn drop(&mut self) {
        let mut record = self.record.lock();
        while let Some((_base, (layout, phys))) = record.pop_first() {
            if let Some(phys) = phys {
                let ptr = phys.to_laddr(minfo::ID_OFFSET);
                unsafe { alloc::alloc::dealloc(*ptr, layout) };
            }
        }
    }
}

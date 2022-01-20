use alloc::{collections::BTreeMap, sync::Arc};
use core::{alloc::Layout, ptr::NonNull};

use bitop_ex::BitOpEx;
use canary::Canary;
use collection_ex::RangeSet;
use paging::LAddr;
use spin::Mutex;

use super::{paging_error, AllocType, ArchSpace, Flags};
use crate::mem::space::Phys;

#[derive(Debug)]
pub struct Allocator {
    canary: Canary<Allocator>,
    free_range: Mutex<RangeSet<LAddr>>,
    record: Mutex<BTreeMap<LAddr, Arc<Phys>>>,
}

impl Allocator {
    pub const fn new(free_range: RangeSet<LAddr>) -> Self {
        Allocator {
            canary: Canary::new(),
            free_range: Mutex::new(free_range),
            record: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn allocate(
        &self,
        ty: AllocType,
        phys: &mut Option<Arc<Phys>>,
        flags: Flags,
        arch: &ArchSpace,
    ) -> solvent::Result<NonNull<[u8]>> {
        self.canary.assert();

        // Get the virtual address.
        // `prefix` and `suffix` are the gaps beside the allocated address range.
        let mut range = self.free_range.lock();

        let (layout, prefix, virt, suffix) = match ty {
            AllocType::Layout(layout) => {
                // Calculate the real size used.
                let layout = layout.align_to(paging::PAGE_LAYOUT.align()).unwrap();
                debug_assert!(!matches!(&phys, Some(phys) if phys.layout() != layout));

                if phys
                    .as_ref()
                    .map_or(false, |phys| phys.base().contains_bit(layout.align() - 1))
                {
                    return Err(solvent::Error::EINVAL);
                }

                let (prefix, virt, suffix) = {
                    let size = layout.pad_to_align().size();

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
                    res.ok_or(solvent::Error::EBUSY)?
                };
                (layout, prefix, virt, suffix)
            }
            AllocType::Virt(virt) => {
                let size = unsafe { virt.end.offset_from(*virt.start) } as usize;
                let layout = Layout::from_size_align(size, paging::PAGE_SIZE)
                    .map_err(solvent::Error::from)?;

                let (prefix, suffix) = {
                    let res = range.range_iter().find_map(|r| {
                        (r.start <= virt.start && virt.end <= r.end)
                            .then_some((r.start..virt.start, virt.end..r.end))
                    });

                    res.ok_or(solvent::Error::EBUSY)?
                };
                (layout, prefix, virt, suffix)
            }
        };

        // Get the physical address mapped to.
        let new_phys = match phys {
            Some(phys) => Arc::clone(phys),
            None => Phys::allocate(layout, flags)?,
        };

        // Map it.
        let base = virt.start;
        arch.maps(virt, new_phys.base(), flags)
            .map_err(paging_error)?;

        range.remove(prefix.start);
        if !prefix.is_empty() {
            let _ = range.insert(prefix);
        }
        if !suffix.is_empty() {
            let _ = range.insert(suffix);
        }
        drop(range);

        let ret =
            unsafe { NonNull::slice_from_raw_parts(base.as_non_null().unwrap(), layout.size()) };
        self.record.lock().insert(base, Arc::clone(&new_phys));
        *phys = Some(new_phys);
        Ok(ret)
    }

    pub unsafe fn modify(
        &self,
        mut ptr: NonNull<[u8]>,
        flags: Flags,
        arch: &ArchSpace,
    ) -> solvent::Result {
        self.canary.assert();

        let virt = {
            let ptr = ptr.as_mut().as_mut_ptr_range();
            LAddr::new(ptr.start)..LAddr::new(ptr.end)
        };

        arch.reprotect(virt, flags).map_err(paging_error)
    }

    pub unsafe fn deallocate(
        &self,
        ptr: NonNull<u8>,
        arch: &ArchSpace,
    ) -> solvent::Result<Arc<Phys>> {
        self.canary.assert();

        // Get the virtual address range from the given memory block.
        let virt_start = LAddr::from(ptr);
        let phys = {
            let mut record = self.record.lock();
            match record.remove(&virt_start) {
                Some(phys) => phys,
                None => return Err(solvent::Error::EINVAL),
            }
        };
        let mut virt = virt_start..LAddr::new(virt_start.add(phys.layout().pad_to_align().size()));

        // Unmap the virtual address & get the physical address.
        let _ = arch.unmaps(virt.clone()).map_err(paging_error)?;

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
        range
            .insert(virt)
            .map_or(Err(solvent::Error::EBUSY), |_| Ok(phys))
    }

    /// The manual dropping function, replacing `Drop::drop` with `arch`.
    ///
    /// # Safety
    ///
    /// This function is called only inside `<space::Space as Drop>::drop`.
    pub(super) unsafe fn dispose(&self, arch: &ArchSpace) {
        let mut record = self.record.lock();
        while let Some((base, phys)) = record.pop_first() {
            let virt = base.to_range(phys.layout());
            let _ = arch.unmaps(virt);
        }
    }
}

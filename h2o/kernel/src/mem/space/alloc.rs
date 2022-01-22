use alloc::sync::Arc;
use core::{alloc::Layout, mem, ops::Range, ptr::NonNull};

use bitop_ex::BitOpEx;
use canary::Canary;
use collection_ex::RangeMap;
use paging::LAddr;
use spin::Mutex;

use super::{paging_error, AllocType, ArchSpace, Flags};
use crate::mem::space::Phys;

#[derive(Debug)]
pub struct Allocator {
    canary: Canary<Allocator>,
    range: Mutex<RangeMap<usize, Arc<Phys>>>,
}

impl Allocator {
    pub const fn new(range: RangeMap<usize, Arc<Phys>>) -> Self {
        Allocator {
            canary: Canary::new(),
            range: Mutex::new(range),
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
        let mut range = self.range.lock();

        let (phys_layout, phys_base) = phys
            .as_ref()
            .map(|phys| (phys.layout(), phys.base()))
            .unzip();
        let mut new_phys = |layout, virt: Range<LAddr>| {
            // Get the physical address mapped to.
            let new_phys = match phys {
                Some(phys) => Arc::clone(phys),
                None => Phys::allocate(layout, flags)?,
            };

            // Map it.
            arch.maps(virt, new_phys.base(), flags)
                .map_err(paging_error)?;
            Ok(new_phys)
        };

        let ret = match ty {
            AllocType::Layout(layout) => {
                // Calculate the real size used.
                let layout = layout.align_to(paging::PAGE_LAYOUT.align()).unwrap();
                debug_assert!(!matches!(phys_layout, Some(l) if l != layout));

                if phys_base.map_or(false, |base| base.contains_bit(layout.align() - 1)) {
                    return Err(solvent::Error::EINVAL);
                }

                let size = layout.pad_to_align().size();

                range
                    .allocate_with(
                        size,
                        |range| new_phys(layout, LAddr::from(range.start)..LAddr::from(range.end)),
                        solvent::Error::ENOMEM,
                    )
                    .map(|(start, phys)| (layout, LAddr::from(start), Arc::clone(phys)))
            }
            AllocType::Virt(virt) => {
                let size = unsafe { virt.end.offset_from(*virt.start) } as usize;
                let layout = Layout::from_size_align(size, paging::PAGE_SIZE)
                    .map_err(solvent::Error::from)?;

                let start = virt.start;
                range
                    .try_insert_with(
                        virt.start.val()..virt.end.val(),
                        || new_phys(layout, virt),
                        solvent::Error::EEXIST,
                    )
                    .map(|phys| (layout, start, Arc::clone(phys)))
            }
        };

        drop(range);

        ret.map(|(layout, base, new_phys)| {
            *phys = Some(new_phys);
            unsafe { NonNull::slice_from_raw_parts(base.as_non_null().unwrap(), layout.size()) }
        })
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
            let mut record = self.range.lock();
            match record.remove(&virt_start.val()) {
                Some(phys) => phys,
                None => return Err(solvent::Error::EINVAL),
            }
        };
        let virt = virt_start..LAddr::new(virt_start.add(phys.layout().pad_to_align().size()));

        // Unmap the virtual address & get the physical address.
        let _ = arch.unmaps(virt).map_err(paging_error)?;

        Ok(phys)
    }

    /// The manual dropping function, replacing `Drop::drop` with `arch`.
    ///
    /// # Safety
    ///
    /// This function is called only inside `<space::Space as Drop>::drop`.
    pub(super) unsafe fn dispose(&self, arch: &ArchSpace) {
        let record = mem::take(&mut *self.range.lock());
        for (_base, (range, _phys)) in record {
            let virt = LAddr::from(range.start)..LAddr::from(range.end);
            let _ = arch.unmaps(virt);
        }
    }
}

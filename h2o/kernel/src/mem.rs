pub mod space;

use alloc::alloc::Global;
use core::{alloc::Allocator, ptr::NonNull};

use paging::LAddr;

#[inline(never)]
unsafe fn alloc_pages(n: usize) -> Option<NonNull<[heap::Page]>> {
    let laddr = pmm::alloc_pages_exact(n, None)?.to_laddr(minfo::ID_OFFSET);
    let ptr = NonNull::new(laddr.cast::<heap::Page>());
    ptr.map(|ptr| NonNull::slice_from_raw_parts(ptr, n))
}

#[inline(never)]
unsafe fn dealloc_pages(pages: NonNull<[heap::Page]>) {
    let paddr = LAddr::new(pages.as_ptr().cast()).to_paddr(minfo::ID_OFFSET);
    let n = pages.len();
    pmm::dealloc_pages_exact(n, paddr);
}

pub fn alloc_system_stack() -> Option<NonNull<u8>> {
    let layout = crate::sched::task::DEFAULT_STACK_LAYOUT;
    Global
        .allocate(layout)
        .ok()
        .and_then(|ptr| NonNull::new(unsafe { ptr.as_mut_ptr().add(layout.size()) }))
}

/// Initialize the PMM and the kernel heap (Rust global allocator).
pub fn init() {
    let all_available = pmm::init(
        crate::KARGS.efi_mmap_paddr,
        crate::KARGS.efi_mmap_len,
        crate::KARGS.efi_mmap_unit,
        minfo::TRAMPOLINE_RANGE,
    );
    log::info!(
        "Memory size: {:.3} GB ({:#x} Bytes)",
        (all_available as f64) / 1073741824.0,
        all_available
    );
    unsafe { heap::set_alloc(alloc_pages, dealloc_pages) };
    heap::test(archop::rand::get() as usize);
}

mod syscall {
    use core::{alloc::Layout, ptr::NonNull};

    use bitop_ex::BitOpEx;
    use solvent::*;

    use super::space;
    use crate::syscall::{InOut, UserPtr};

    fn check_options(size: usize, align: usize, flags: u32) -> Result<(Layout, space::Flags)> {
        if size.contains_bit(paging::PAGE_MASK) || !align.is_power_of_two() {
            return Err(Error(EINVAL));
        }
        let layout = Layout::from_size_align(size, align).map_err(|_| Error(EINVAL))?;
        let flags = space::Flags::from_bits(flags).ok_or(Error(EINVAL))?;

        Ok((layout, flags))
    }

    #[syscall]
    fn virt_alloc(
        virt_ptr: UserPtr<InOut, *mut u8>,
        phys: usize,
        size: usize,
        align: usize,
        flags: u32,
    ) -> Handle {
        let (layout, flags) = check_options(size, align, flags)?;
        let ptr = unsafe { virt_ptr.r#in().read()? };

        // TODO: Check whether the physical address is permitted.
        let phys = (phys != 0).then_some(paging::PAddr::new(phys));
        let phys = phys.map(|phys| space::Phys::new(phys, layout, flags));

        let ty = if ptr.is_null() {
            space::AllocType::Layout(layout)
        } else {
            space::AllocType::Virt(
                paging::LAddr::new(ptr)..paging::LAddr::new(unsafe { ptr.add(size) }),
            )
        };

        let ret = space::with_current(|cur| cur.allocate(ty, phys, flags));
        ret.map_err(Into::into).and_then(|virt| {
            let ptr = virt.as_ptr().as_mut_ptr();
            unsafe { virt_ptr.out().write(ptr) }.unwrap();
            crate::sched::SCHED
                .with_current(|cur| {
                    let info = cur.tid().info();
                    unsafe { info.handles().write().insert_unchecked(box virt) }
                })
                .ok_or(Error(ESRCH))
        })
    }

    #[syscall]
    fn virt_modify(hdl: Handle, ptr: *mut u8, size: usize, flags: u32) {
        hdl.check_null()?;

        if size.contains_bit(paging::PAGE_MASK) {
            return Err(Error(EINVAL));
        }
        let flags = space::Flags::from_bits(flags).ok_or(Error(EINVAL))?;
        let ptr = NonNull::new(ptr).ok_or(Error(EINVAL))?;
        let ptr = NonNull::slice_from_raw_parts(ptr, size);

        crate::sched::SCHED
            .with_current(|cur| {
                let info = cur.tid().info();
                match unsafe { info.handles().read().get_unchecked::<space::Virt>(hdl) } {
                    Some(virt) => unsafe { virt.modify(ptr, flags) }.map_err(Into::into),
                    None => Err(Error(EINVAL)),
                }
            })
            .unwrap_or(Err(Error(ESRCH)))
    }

    #[syscall]
    fn mem_alloc(size: usize, align: usize, flags: u32) -> *mut u8 {
        let (layout, flags) = check_options(size, align, flags)?;
        let ty = space::AllocType::Layout(layout);
        let ret = space::with_current(|cur| cur.allocate(ty, None, flags));
        ret.map_err(Into::into).map(|virt| virt.leak().as_mut_ptr())
    }

    #[syscall]
    fn mem_dealloc(ptr: *mut u8) {
        let ret = unsafe {
            let ptr = NonNull::new(ptr).ok_or(Error(EINVAL))?;
            space::with_current(|cur| cur.deallocate(ptr))
        };
        ret.map_err(Into::into).map(|_| {})
    }
}

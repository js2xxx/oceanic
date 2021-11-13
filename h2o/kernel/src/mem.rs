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
    use bitop_ex::BitOpEx;
    use solvent::*;

    #[syscall]
    fn alloc_pages(virt: *mut u8, phys: usize, size: usize, align: usize, flags: u32) -> *mut u8 {
        use core::{alloc::Layout, ptr::NonNull};

        use super::space;

        if size.contains_bit(paging::PAGE_MASK) || !align.is_power_of_two() {
            return Err(Error(EINVAL));
        }
        let layout = Layout::from_size_align(size, align).map_err(|_| Error(EINVAL))?;

        let flags = space::Flags::from_bits(flags).ok_or(Error(EINVAL))?;

        // TODO: Check whether the physical address is permitted.
        let phys = (phys != 0).then_some(paging::PAddr::new(phys));
        let phys = phys.map(|phys| space::Phys::new(phys, layout, flags));

        let ty = if virt.is_null() {
            space::AllocType::Layout(layout)
        } else {
            // TODO: Check whether the virtual address is permitted.
            space::AllocType::Virt(
                paging::LAddr::new(virt)..paging::LAddr::new(unsafe { virt.add(size) }),
            )
        };

        let ret = space::with_current(|cur| cur.allocate(ty, phys, flags));
        ret.map_err(Into::into).map(NonNull::as_mut_ptr)
    }

    #[syscall]
    fn dealloc_pages(ptr: *mut u8) {
        use core::ptr::NonNull;

        use super::space;

        let ret = unsafe {
            let ptr = NonNull::new(ptr).ok_or(Error(EINVAL))?;
            space::with_current(|cur| cur.deallocate(ptr))
        };
        ret.map_err(Into::into)
    }

    #[syscall]
    fn modify_pages(ptr: *mut u8, size: usize, flags: u32) {
        use core::ptr::NonNull;

        use super::space;

        if size.contains_bit(paging::PAGE_MASK) {
            return Err(Error(EINVAL));
        }
        let flags = space::Flags::from_bits(flags).ok_or(Error(EINVAL))?;

        let ret = unsafe {
            let ptr = NonNull::new(ptr).ok_or(Error(EINVAL))?;
            let ptr = NonNull::slice_from_raw_parts(ptr, size);
            space::with_current(|cur| cur.modify(ptr, flags))
        };
        ret.map_err(Into::into)?;
        Ok(())
    }
}

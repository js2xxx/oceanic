mod arena;
pub mod heap;
pub mod space;

use alloc::alloc::Global;
use core::{alloc::Allocator, ptr::NonNull};

use iter_ex::PointerIterator;
use spin::Lazy;

pub use self::arena::Arena;
use crate::kargs;

pub static MMAP: Lazy<PointerIterator<pmm::boot::MemRange>> = Lazy::new(|| {
    PointerIterator::new(
        kargs().efi_mmap_paddr.to_laddr(minfo::ID_OFFSET).cast(),
        kargs().efi_mmap_len,
        kargs().efi_mmap_unit,
    )
});

pub fn alloc_system_stack() -> solvent::Result<NonNull<u8>> {
    let layout = crate::sched::task::DEFAULT_STACK_LAYOUT;
    Global
        .allocate(layout)
        .map(|ptr| unsafe { NonNull::new_unchecked(ptr.as_mut_ptr().add(layout.size())) })
        .map_err(Into::into)
}

/// Initialize the PMM and the kernel heap (Rust global allocator).
pub fn init() {
    let all_available = pmm::init(&*MMAP, minfo::TRAMPOLINE_RANGE);
    log::info!(
        "Memory size: {:.3} GB ({:#x} Bytes)",
        (all_available as f64) / 1073741824.0,
        all_available
    );
    heap::test_global();
    unsafe { space::init() };
}

mod syscall {
    use core::{alloc::Layout, ptr::NonNull};

    use bitop_ex::BitOpEx;
    use solvent::*;

    use super::space;

    fn check_options(size: usize, align: usize, flags: u32) -> Result<(Layout, space::Flags)> {
        if size.contains_bit(paging::PAGE_MASK) || !align.is_power_of_two() {
            return Err(Error::EINVAL);
        }
        let layout = Layout::from_size_align(size, align).map_err(|_| Error::EINVAL)?;
        let flags = space::Flags::from_bits(flags).ok_or(Error::EINVAL)?;

        Ok((layout, flags))
    }

    #[syscall]
    fn mem_alloc(size: usize, align: usize, flags: u32) -> Result<*mut u8> {
        let (layout, flags) = check_options(size, align, flags)?;
        let ret = space::with_current(|cur| cur.allocate(layout, flags));
        ret.map_err(Into::into)
            .map(|addr| addr.as_non_null_ptr().as_ptr())
    }

    #[syscall]
    fn mem_dealloc(ptr: *mut u8) -> Result {
        let ret = unsafe {
            let ptr = NonNull::new(ptr).ok_or(Error::EINVAL)?;
            space::with_current(|cur| cur.unmap(ptr))
        };
        ret.map_err(Into::into).map(|_| {})
    }
}

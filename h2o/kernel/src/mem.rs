mod arena;
pub mod heap;
pub mod space;
mod syscall;

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

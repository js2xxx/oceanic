mod arena;
pub mod heap;
pub mod space;
mod syscall;

use alloc::{alloc::Global, sync::Arc};
use core::{alloc::Allocator, ptr::NonNull};

use iter_ex::PointerIterator;
use spin::Lazy;

pub use self::arena::Arena;
use crate::{dev::Resource, kargs};

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

static MEM_RESOURCE: Lazy<Arc<Resource<usize>>> = Lazy::new(|| {
    let (all_available, addr_max) = pmm::init(&*MMAP, minfo::TRAMPOLINE_RANGE);
    log::info!(
        "Memory size: {:.3} GB ({:#x} Bytes)",
        (all_available as f64) / 1073741824.0,
        all_available
    );
    heap::test_global();
    unsafe { space::init() };

    let ret = Resource::new(archop::rand::get(), 0..addr_max, None);
    // Make memory in heap not to be used by devices.
    for mdsc_ptr in &*MMAP {
        let mdsc = unsafe { &*mdsc_ptr };
        if matches!(
            mdsc.ty,
            pmm::boot::MemType::Conventional | pmm::boot::MemType::PersistentMemory
        ) {
            let range = (mdsc.phys as usize)
                ..(mdsc.phys as usize + mdsc.page_count as usize * paging::PAGE_SIZE);
            core::mem::forget(
                ret.allocate(range)
                    .expect("Failed to reserve usable memory"),
            );
        }
    }
    ret
});

#[inline]
pub fn mem_resource() -> &'static Arc<Resource<usize>> {
    &MEM_RESOURCE
}

/// Initialize the PMM and the kernel heap (Rust global allocator).
#[inline]
pub fn init() {
    Lazy::force(&MEM_RESOURCE);
}

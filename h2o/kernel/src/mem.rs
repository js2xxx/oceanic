mod arena;
pub mod heap;
pub mod space;
mod syscall;

use alloc::{alloc::Global, sync::Arc};
use core::{alloc::Allocator, ptr::NonNull, sync::atomic::AtomicUsize};

use archop::Azy;
use iter_ex::PtrIter;
use sv_call::{mem::Flags, Feature};

pub use self::arena::Arena;
use crate::{dev::Resource, kargs};

pub static MMAP: Azy<PtrIter<pmm::boot::MemRange>> = Azy::new(|| {
    PtrIter::new(
        kargs().efi_mmap_paddr.to_laddr(minfo::ID_OFFSET).cast(),
        kargs().efi_mmap_len,
        kargs().efi_mmap_unit,
    )
});

static ALL_AVAILABLE: AtomicUsize = AtomicUsize::new(0);

static MEM_RESOURCE: Azy<Arc<Resource<usize>>> = Azy::new(|| {
    let (all_available, addr_max) = pmm::init(&*MMAP, minfo::TRAMPOLINE_RANGE);
    log::info!(
        "Memory size: {:.3} GB ({:#x} Bytes)",
        (all_available as f64) / 1073741824.0,
        all_available
    );
    ALL_AVAILABLE.store(all_available, core::sync::atomic::Ordering::SeqCst);
    heap::test_global();
    unsafe { space::init() };

    let ret = Resource::new_root(archop::rand::get(), 0..addr_max);
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

pub fn alloc_system_stack() -> sv_call::Result<NonNull<u8>> {
    let layout = crate::sched::task::DEFAULT_STACK_LAYOUT;
    Global
        .allocate(layout)
        .map(|ptr| unsafe { NonNull::new_unchecked(ptr.as_mut_ptr().add(layout.size())) })
        .map_err(Into::into)
}

/// Initialize the PMM and the kernel heap (Rust global allocator).
#[inline]
pub fn init() {
    Azy::force(&MEM_RESOURCE);
}

pub fn flags_to_features(flags: Flags) -> Feature {
    let mut feat = Feature::SEND | Feature::SYNC;
    if flags.contains(Flags::READABLE) {
        feat |= Feature::READ
    }
    if flags.contains(Flags::WRITABLE) {
        feat |= Feature::WRITE
    }
    if flags.contains(Flags::EXECUTABLE) {
        feat |= Feature::EXECUTE
    }
    feat
}

pub fn features_to_flags(feat: Feature, user: bool) -> Flags {
    let mut flags = Flags::empty();
    if user {
        flags |= Flags::USER_ACCESS;
    }
    if feat.contains(Feature::READ) {
        flags |= Flags::READABLE;
    }
    if feat.contains(Feature::WRITE) {
        flags |= Flags::WRITABLE;
    }
    if feat.contains(Feature::EXECUTE) {
        flags |= Flags::EXECUTABLE;
    }
    flags
}

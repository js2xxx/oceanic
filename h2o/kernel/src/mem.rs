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
    use alloc::sync::Arc;
    use core::{alloc::Layout, ptr::NonNull};

    use bitop_ex::BitOpEx;
    use solvent::{mem::MapInfo, *};

    use super::space;
    use crate::{
        sched::{PREEMPT, SCHED},
        syscall::{In, UserPtr},
    };

    fn check_layout(size: usize, align: usize) -> Result<Layout> {
        if size.contains_bit(paging::PAGE_MASK) || !align.is_power_of_two() {
            return Err(Error::EINVAL);
        }
        Layout::from_size_align(size, align).map_err(Error::from)
    }

    fn check_flags(flags: u32) -> Result<space::Flags> {
        let flags = space::Flags::from_bits(flags).ok_or(Error::EINVAL)?;
        if !flags.contains(space::Flags::USER_ACCESS) {
            return Err(Error::EPERM);
        }
        Ok(flags)
    }

    #[syscall]
    fn phys_alloc(size: usize, align: usize, flags: u32) -> Result<Handle> {
        let layout = check_layout(size, align)?;
        let flags = check_flags(flags)?;
        let phys = PREEMPT.scope(|| space::Phys::allocate(layout, flags))?;
        SCHED.with_current(|cur| cur.tid().handles().insert(phys))
    }

    #[syscall]
    fn mem_map(mi: UserPtr<In, MapInfo>) -> Result<*mut u8> {
        let mi = unsafe { mi.read() }?;
        let flags = check_flags(mi.flags.bits())?;
        let phys = SCHED.with_current(|cur| {
            cur.tid()
                .handles()
                .get::<Arc<space::Phys>>(mi.phys)
                .map(|obj| Arc::clone(obj))
        })?;
        space::with_current(|cur| {
            let offset = if mi.map_addr {
                Some(mi.addr.checked_sub(cur.range.start).ok_or(Error::ERANGE)?)
            } else {
                None
            };
            cur.map(offset, phys, mi.phys_offset, mi.len, flags)
        })
        .map(|addr| *addr)
    }

    #[syscall]
    fn mem_alloc(size: usize, align: usize, flags: u32) -> Result<*mut u8> {
        let layout = check_layout(size, align)?;
        let flags = check_flags(flags)?;
        let ret = space::with_current(|cur| cur.allocate(layout, flags));
        ret.map_err(Into::into)
            .map(|addr| addr.as_non_null_ptr().as_ptr())
    }

    #[syscall]
    fn mem_unmap(ptr: *mut u8) -> Result {
        unsafe {
            let ptr = NonNull::new(ptr).ok_or(Error::EINVAL)?;
            space::with_current(|cur| cur.unmap(ptr))
        }
    }
}

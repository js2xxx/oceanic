//! # Address space management for H2O.
//!
//! This module is responsible for managing system memory and address space in a
//! higher level, especially for large objects like APIC.

mod alloc;
mod obj;

cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        #[path = "space/x86_64/mod.rs"]
        mod arch;
        pub use arch::{page_fault, ErrCode as PageFaultErrCode};
    }
}

use core::{alloc::Layout, ops::Range, ptr::NonNull};

use ::alloc::sync::Arc;
pub use arch::init_pgc;
use bitop_ex::BitOpEx;
use canary::Canary;
use collection_ex::RangeSet;
pub use obj::*;
use paging::LAddr;
pub use solvent::mem::Flags;
use spin::Lazy;

use crate::sched::{task, PREEMPT};

type ArchSpace = arch::Space;

pub static KRL: Lazy<Arc<Space>> = Lazy::new(|| Space::new(task::Type::Kernel));

#[thread_local]
static mut CURRENT: Option<Arc<Space>> = None;

fn paging_error(err: paging::Error) -> solvent::Error {
    use solvent::Error;
    match err {
        paging::Error::OutOfMemory => Error::ENOMEM,
        paging::Error::AddrMisaligned { .. } => Error::EALIGN,
        paging::Error::RangeEmpty => Error::EBUFFER,
        paging::Error::EntryExistent(b) => {
            if b {
                Error::EEXIST
            } else {
                Error::ENOENT
            }
        }
    }
}

/// The total available range of address space for the create type.
///
/// We cannot simply pass a [`Range`] to [`Space`]'s constructor because without
/// control arbitrary, even incanonical ranges would be passed and cause
/// unrecoverable errors.
fn ty_to_range_set(ty: task::Type) -> RangeSet<LAddr> {
    let range = match ty {
        task::Type::Kernel => minfo::KERNEL_ALLOCABLE_RANGE,
        task::Type::User => LAddr::from(minfo::USER_BASE)..LAddr::from(minfo::USER_END),
    };

    let mut range_set = RangeSet::new();
    let _ = range_set.insert(range);
    range_set
}

#[derive(Debug, Clone)]
pub enum AllocType {
    Layout(Layout),
    Virt(Range<LAddr>),
}

/// The structure that represents an address space.
///
/// The address space is defined from the concept of the virtual addressing in
/// CPU. It's arch- specific responsibility to map the virtual address to the
/// real (physical) address in RAM. This structure is used to allocate & reserve
/// address space ranges for various requests.
///
/// TODO: Support the requests for reserving address ranges.
#[derive(Debug)]
pub struct Space {
    canary: Canary<Space>,
    ty: task::Type,

    /// The arch-specific part of the address space.
    arch: ArchSpace,

    /// The general allocator.
    allocator: Arc<alloc::Allocator>,
}

unsafe impl Send for Space {}
unsafe impl Sync for Space {}

impl Space {
    /// Create a new address space.
    pub fn new(ty: task::Type) -> Arc<Self> {
        Arc::new(Space {
            canary: Canary::new(),
            ty,
            arch: ArchSpace::new(),
            allocator: Arc::new(alloc::Allocator::new(ty_to_range_set(ty))),
        })
    }

    /// Allocate an address range in the space.
    pub fn allocate(
        self: &Arc<Self>,
        ty: AllocType,
        mut phys: Option<Arc<Phys>>,
        flags: Flags,
    ) -> solvent::Result<Virt> {
        self.canary.assert();

        PREEMPT.scope(|| {
            self.allocator
                .allocate(ty.clone(), &mut phys, flags, &self.arch)
                .map(|ptr| Virt::new(self.ty, ptr, phys.unwrap(), Arc::clone(self)))
        })
    }

    /// Allocate an address range in the kernel space.
    ///
    /// Used for sharing kernel variables of [`KernelVirt`].
    pub fn allocate_kernel(
        self: &Arc<Self>,
        ty: AllocType,
        phys: Option<Arc<Phys>>,
        flags: Flags,
    ) -> solvent::Result<KernelVirt> {
        self.canary.assert();
        match self.ty {
            task::Type::Kernel => self
                .allocate(ty, phys, flags)
                .map(|virt| KernelVirt::new(virt).unwrap()),
            task::Type::User => Err(solvent::Error::EPERM),
        }
    }

    /// Get the mapped physical address of the specified pointer.
    pub fn get(&self, ptr: NonNull<u8>) -> solvent::Result<paging::PAddr> {
        self.arch.query(LAddr::from(ptr)).map_err(paging_error)
    }

    /// Modify the access flags of an address range without a specific type.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `ptr` was allocated by this `Space` and no
    /// pointers or references within the address range are present (or will be
    /// influenced by the modification).
    pub unsafe fn modify(&self, ptr: NonNull<[u8]>, flags: Flags) -> solvent::Result {
        self.canary.assert();

        PREEMPT.scope(|| self.allocator.modify(ptr, flags, &self.arch))
    }

    /// Deallocate an address range in the space without a specific type.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `ptr` was allocated by this `Space`.
    pub unsafe fn deallocate(&self, ptr: NonNull<u8>) -> solvent::Result<Arc<Phys>> {
        self.canary.assert();

        PREEMPT.scope(|| self.allocator.deallocate(ptr, &self.arch))
    }

    /// # Safety
    ///
    /// The caller must ensure that loading the space is safe and not cause any
    /// #PF.
    pub unsafe fn load(&self) {
        self.canary.assert();
        self.arch.load()
    }

    pub fn init_stack(self: &Arc<Self>, size: usize) -> solvent::Result<LAddr> {
        self.canary.assert();

        let cnt = size.div_ceil_bit(paging::PAGE_SHIFT);
        let (layout, _) = paging::PAGE_LAYOUT.repeat(cnt + 2)?;

        let virt = self.allocate(
            AllocType::Layout(layout),
            None,
            Flags::READABLE | Flags::WRITABLE | Flags::USER_ACCESS,
        )?;
        let base = virt.base();
        let actual_end = unsafe { NonNull::new_unchecked(base.add(paging::PAGE_SIZE * (cnt + 1))) };

        let prefix =
            NonNull::slice_from_raw_parts(virt.as_ptr().as_non_null_ptr(), paging::PAGE_SIZE);
        let suffix = NonNull::slice_from_raw_parts(actual_end, paging::PAGE_SIZE);

        unsafe {
            virt.modify(prefix, Flags::READABLE)?;
            virt.modify(suffix, Flags::READABLE)?;
        }

        core::mem::forget(virt);

        Ok(LAddr::from(actual_end))
    }
}

impl Drop for Space {
    fn drop(&mut self) {
        PREEMPT.scope(|| unsafe { self.allocator.dispose(&self.arch) })
    }
}

/// Load the kernel space for enery CPU.
///
/// # Safety
///
/// The function must be called only once from each application CPU.
pub unsafe fn init() {
    let space = Arc::clone(&KRL);
    unsafe { space.load() };
    CURRENT = Some(space);
}

/// Get the reference of the per-CPU current space without lock.
///
/// # Safety
///
/// The caller must ensure that [`CURRENT`] will not be modified where the
/// reference is alive.
pub unsafe fn current<'a>() -> &'a Arc<Space> {
    unsafe { CURRENT.as_ref().expect("No current space available") }
}

/// Get the reference of the per-CPU current space.
pub fn with_current<'a, F, R>(func: F) -> R
where
    F: FnOnce(&'a Arc<Space>) -> R,
    R: 'a,
{
    let cur = unsafe { CURRENT.as_ref().expect("No current space available") };
    func(cur)
}

pub unsafe fn with<F, R>(space: &Arc<Space>, func: F) -> R
where
    F: FnOnce(&Arc<Space>) -> R,
{
    PREEMPT.scope(|| {
        let old = set_current(Arc::clone(space));
        let ret = func(space);
        set_current(old);
        ret
    })
}

/// Set the current memory space of the current CPU.
///
/// # Safety
///
/// The function must be called only from the epilogue of context switching.
pub unsafe fn set_current(space: Arc<Space>) -> Arc<Space> {
    PREEMPT.scope(|| {
        if !Arc::ptr_eq(current(), &space) {
            space.load();
            CURRENT.replace(space).unwrap()
        } else {
            space
        }
    })
}

//! # Address space management for H2O.
//!
//! This module is responsible for managing system memory and address space in a
//! higher level, especially for large objects like APIC.

mod phys;
mod virt;

cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        #[path = "space/x86_64/mod.rs"]
        mod arch;
        pub use self::arch::{page_fault, ErrCode as PageFaultErrCode};
    }
}

use alloc::sync::{Arc, Weak};
use core::{
    alloc::Layout,
    ops::{Deref, Range},
    ptr::NonNull,
};

use archop::Azy;
use bitop_ex::BitOpEx;
use paging::{LAddr, PAGE_SHIFT};
use spin::Mutex;
pub use sv_call::mem::Flags;
use sv_call::mem::PhysOptions;

pub use self::{arch::init_pgc, phys::*, virt::*};
use crate::sched::{task, PREEMPT};

type ArchSpace = arch::Space;

pub static KRL: Azy<Arc<Space>> =
    Azy::new(|| Space::try_new(task::Type::Kernel).expect("Failed to create kernel space"));

#[thread_local]
static mut CURRENT: Option<Arc<Space>> = None;

fn paging_error(err: paging::Error) -> sv_call::Error {
    use sv_call::*;
    match err {
        paging::Error::OutOfMemory => ENOMEM,
        paging::Error::AddrMisaligned { .. } => EALIGN,
        paging::Error::RangeEmpty => EBUFFER,
        paging::Error::EntryExistent(b) => {
            if b {
                EEXIST
            } else {
                ENOENT
            }
        }
    }
}

/// The total available range of address space for the create type.
///
/// We cannot simply pass a [`Range`] to [`Space`]'s constructor because without
/// control arbitrary, even incanonical ranges would be passed and cause
/// unrecoverable errors.
fn ty_to_range(ty: task::Type) -> Range<usize> {
    match ty {
        task::Type::Kernel => minfo::KERNEL_ALLOCABLE_RANGE,
        task::Type::User => minfo::USER_BASE..minfo::USER_END,
    }
}

#[inline]
pub fn page_aligned(size: usize) -> Layout {
    unsafe { Layout::from_size_align_unchecked(size, paging::PAGE_LAYOUT.align()) }
}

#[derive(Debug)]
pub struct Space {
    arch: ArchSpace,
    root: Arc<Virt>,
    vdso: Mutex<Option<LAddr>>,
}

unsafe impl Send for Space {}
unsafe impl Sync for Space {}

impl Space {
    /// Create a new address space.
    pub fn try_new(ty: task::Type) -> sv_call::Result<Arc<Self>> {
        Ok(Arc::new_cyclic(|me| Space {
            arch: ArchSpace::new(),
            root: Virt::new_root(ty, Weak::clone(me)),
            vdso: Mutex::new(None),
        }))
    }

    pub fn root(&self) -> &Arc<Virt> {
        &self.root
    }
}

impl Deref for Space {
    type Target = Arc<Virt>;

    fn deref(&self) -> &Self::Target {
        &self.root
    }
}

pub(crate) fn allocate(size: usize, flags: Flags, zeroed: bool) -> sv_call::Result<NonNull<[u8]>> {
    let phys = allocate_phys(
        size.round_up_bit(PAGE_SHIFT),
        if zeroed {
            PhysOptions::ZEROED
        } else {
            Default::default()
        },
        false,
    )?;
    let len = phys.len();

    KRL.root
        .map(None, phys, 0, page_aligned(len), flags)
        .map(|addr| {
            let ptr = unsafe { NonNull::new_unchecked(*addr) };
            NonNull::slice_from_raw_parts(ptr, len)
        })
}

pub(crate) unsafe fn unmap(ptr: NonNull<u8>) -> sv_call::Result {
    let base = LAddr::from(ptr);
    PREEMPT.scope(|| {
        let ret = KRL.root.children.lock().remove(&base);
        ret.map_or(Err(sv_call::ENOENT), |child| {
            let end = child.end(base);
            let _ = KRL.arch.unmaps(base..end);
            Ok(())
        })
    })
}

pub fn init_stack(virt: &Arc<Virt>, size: usize) -> sv_call::Result<LAddr> {
    let flags = Flags::READABLE | Flags::WRITABLE | Flags::USER_ACCESS;
    let virt = virt.allocate(None, unsafe {
        Layout::from_size_align_unchecked(paging::PAGE_SIZE * 2 + size, paging::PAGE_SIZE)
    })?;
    let phys = allocate_phys(size, Default::default(), false)?;
    let ret = virt.upgrade().unwrap().map(
        Some(paging::PAGE_SIZE),
        phys,
        0,
        unsafe { Layout::from_size_align_unchecked(size, paging::PAGE_SIZE) },
        flags,
    )?;

    Ok(LAddr::from(ret.val() + size))
}

/// Load the kernel space for enery CPU.
///
/// # Safety
///
/// The function must be called only once from each application CPU.
pub unsafe fn init() {
    let space = Arc::clone(&KRL);
    unsafe { space.arch.load() };
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
#[inline]
pub fn with_current<'a, F, R>(func: F) -> R
where
    F: FnOnce(&'a Arc<Space>) -> R,
    R: 'a,
{
    PREEMPT.scope(|| {
        let cur = unsafe { CURRENT.as_ref().expect("No current space available") };
        func(cur)
    })
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
            space.arch.load();
            CURRENT.replace(space).expect("No current space available")
        } else {
            space
        }
    })
}

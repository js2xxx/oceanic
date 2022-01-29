//! # Address space management for H2O.
//!
//! This module is responsible for managing system memory and address space in a
//! higher level, especially for large objects like APIC.

mod obj;

cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        #[path = "space/x86_64/mod.rs"]
        mod arch;
        pub use self::arch::{page_fault, ErrCode as PageFaultErrCode};
    }
}

use core::{
    alloc::Layout,
    mem,
    ops::{Add, Range},
    ptr::NonNull,
};

use ::alloc::sync::Arc;
use archop::Azy;
use bitop_ex::BitOpEx;
use canary::Canary;
use collection_ex::RangeMap;
use paging::{LAddr, PAddr, PAGE_LAYOUT};
use spin::Mutex;
pub use sv_call::mem::Flags;

pub use self::{arch::init_pgc, obj::*};
use crate::sched::{task, PREEMPT};

type ArchSpace = arch::Space;

pub static KRL: Azy<Arc<Space>> = Azy::new(|| Space::new(task::Type::Kernel));

#[thread_local]
static mut CURRENT: Option<Arc<Space>> = None;

fn paging_error(err: paging::Error) -> sv_call::Error {
    use sv_call::Error;
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
fn ty_to_range(ty: task::Type) -> Range<usize> {
    match ty {
        task::Type::Kernel => minfo::KERNEL_ALLOCABLE_RANGE,
        task::Type::User => minfo::USER_BASE..minfo::USER_END,
    }
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
    pub(super) range: Range<usize>,
    map: Mutex<RangeMap<usize, Arc<Phys>>>,
}

unsafe impl Send for Space {}
unsafe impl Sync for Space {}

impl Space {
    /// Create a new address space.
    pub fn new(ty: task::Type) -> Arc<Self> {
        let range = ty_to_range(ty);
        Arc::new(Space {
            canary: Canary::new(),
            ty,
            arch: ArchSpace::new(),
            range: range.clone(),
            map: Mutex::new(RangeMap::new(range)),
        })
    }

    #[inline]
    pub fn ty(&self) -> task::Type {
        self.ty
    }

    /// Shorthand for `Phys::allocate` + `Space::map`.
    pub fn allocate(&self, layout: Layout, flags: Flags) -> sv_call::Result<NonNull<[u8]>> {
        self.canary.assert();

        let phys = Phys::allocate(layout, flags)?;
        let layout = phys.layout();

        self.map(None, phys, 0, layout.size(), flags).map(|addr| {
            let ptr = unsafe { NonNull::new_unchecked(*addr) };
            NonNull::slice_from_raw_parts(ptr, layout.size())
        })
    }

    #[inline]
    pub fn map_addr(
        &self,
        virt: Range<LAddr>,
        phys: Option<Arc<Phys>>,
        flags: Flags,
    ) -> sv_call::Result {
        self.canary.assert();

        let offset = virt
            .start
            .val()
            .checked_sub(self.range.start)
            .ok_or(sv_call::Error::ERANGE)?;
        let len = virt
            .end
            .val()
            .checked_sub(virt.start.val())
            .ok_or(sv_call::Error::ERANGE)?;
        let phys = match phys {
            Some(phys) => phys,
            None => Phys::allocate(Layout::from_size_align(len, PAGE_LAYOUT.align())?, flags)?,
        };
        self.map(Some(offset), phys, 0, len, flags & !Flags::ZEROED)
            .map(|_| {})
    }

    /// Map a physical memory to a virtual address.
    pub fn map(
        &self,
        offset: Option<usize>,
        phys: Arc<Phys>,
        phys_offset: usize,
        len: usize,
        flags: Flags,
    ) -> sv_call::Result<LAddr> {
        self.canary.assert();

        if flags & !phys.flags() != Flags::empty() || flags.contains(Flags::ZEROED) {
            return Err(sv_call::Error::EPERM);
        }

        let phys_offset_end = phys_offset.wrapping_add(len);
        if !(phys_offset < phys_offset_end && phys_offset_end <= phys.layout().size()) {
            return Err(sv_call::Error::ERANGE);
        }

        let phys_start = PAddr::new(phys.base().add(phys_offset));
        let arch_map = |range: Range<usize>| {
            let virt = LAddr::from(range.start)..LAddr::from(range.end);
            self.arch
                .maps(virt, phys_start, flags)
                .map_err(paging_error)
        };

        if let Some(offset) = offset {
            let start = offset.wrapping_add(self.range.start);
            let end = start.wrapping_add(len);
            if !(self.range.start <= start && start < end && end <= self.range.end) {
                return Err(sv_call::Error::ERANGE);
            }

            PREEMPT.scope(|| {
                self.map.lock().try_insert_with(
                    start..end,
                    || arch_map(start..end).map(|_| (phys, LAddr::from(start))),
                    sv_call::Error::EBUSY,
                )
            })
        } else {
            PREEMPT.scope(|| {
                self.map
                    .lock()
                    .allocate_with(
                        len,
                        |range| arch_map(range).map(|_| (phys, ())),
                        sv_call::Error::ENOMEM,
                    )
                    .map(|(start, _)| LAddr::from(start))
            })
        }
    }

    /// Get the mapped physical address of the specified pointer.
    pub fn get(&self, ptr: NonNull<u8>) -> sv_call::Result<paging::PAddr> {
        self.arch.query(LAddr::from(ptr)).map_err(paging_error)
    }

    /// Modify the access flags of an address range.
    ///
    /// # Safety
    ///
    /// The caller must ensure that no pointers or references within the address
    /// range are present (or will be influenced by the modification).
    pub unsafe fn reprotect(&self, mut ptr: NonNull<[u8]>, flags: Flags) -> sv_call::Result {
        self.canary.assert();

        let virt = {
            let ptr = ptr.as_mut().as_mut_ptr_range();
            LAddr::new(ptr.start)..LAddr::new(ptr.end)
        };

        PREEMPT.scope(|| {
            let map = self.map.lock();
            match map.get_contained_range(virt.start.val()..virt.end.val()) {
                Some(_) => self.arch.reprotect(virt, flags).map_err(paging_error),
                None => Err(sv_call::Error::ENOENT),
            }
        })
    }

    /// Deallocate an address range in the space without a specific type.
    ///
    /// # Safety
    ///
    /// The caller must ensure that no more references are pointing at the
    /// address range to be deallocated.
    pub unsafe fn unmap(&self, ptr: NonNull<u8>) -> sv_call::Result {
        self.canary.assert();

        let ret = PREEMPT.scope(|| self.map.lock().remove(LAddr::from(ptr).val()));
        ret.map_or(Err(sv_call::Error::ENOENT), |(range, _phys)| {
            let _ = PREEMPT.scope(|| {
                self.arch
                    .unmaps(LAddr::from(range.start)..LAddr::from(range.end))
            });
            Ok(())
        })
    }

    /// # Safety
    ///
    /// The caller must ensure that loading the space is safe and not cause any
    /// #PF.
    pub unsafe fn load(&self) {
        self.canary.assert();
        self.arch.load()
    }

    pub fn init_stack(self: &Arc<Self>, size: usize) -> sv_call::Result<LAddr> {
        self.canary.assert();

        let cnt = size.div_ceil_bit(paging::PAGE_SHIFT);
        let (layout, _) = paging::PAGE_LAYOUT.repeat(cnt + 2)?;

        let flags = Flags::READABLE | Flags::WRITABLE | Flags::USER_ACCESS;
        let ptr = self.allocate(layout, flags)?;
        let base = unsafe { ptr.as_non_null_ptr() };
        let actual_end =
            unsafe { NonNull::new_unchecked(base.as_ptr().add(paging::PAGE_SIZE * (cnt + 1))) };

        let prefix = NonNull::slice_from_raw_parts(base, paging::PAGE_SIZE);
        let suffix = NonNull::slice_from_raw_parts(actual_end, paging::PAGE_SIZE);

        unsafe {
            self.reprotect(prefix, Flags::READABLE)?;
            self.reprotect(suffix, Flags::READABLE)?;
        }

        Ok(LAddr::from(actual_end))
    }
}

impl Drop for Space {
    fn drop(&mut self) {
        let map = PREEMPT.scope(|| mem::take(&mut *self.map.lock()));
        for (_, (range, _)) in map {
            let _ = PREEMPT.scope(|| {
                self.arch
                    .unmaps(LAddr::from(range.start)..LAddr::from(range.end))
            });
        }
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
            space.load();
            CURRENT.replace(space).expect("No current space available")
        } else {
            space
        }
    })
}

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
    }
}

use core::{alloc::Layout, ops::Range, ptr::NonNull};

use ::alloc::{
    alloc::{alloc as mem_alloc, dealloc as mem_dealloc},
    collections::BTreeMap,
    sync::Arc,
};
pub use arch::init_pgc;
use bitop_ex::BitOpEx;
use canary::Canary;
use collection_ex::RangeSet;
pub use obj::*;
use paging::LAddr;
pub use solvent::mem::Flags;
use spin::{Lazy, Mutex, MutexGuard};

use crate::sched::{task, PREEMPT};

type ArchSpace = arch::Space;

pub static KRL: Lazy<Arc<Space>> = Lazy::new(|| Space::new(task::Type::Kernel));

#[thread_local]
static mut CURRENT: Option<Arc<Space>> = None;

#[derive(Debug)]
pub enum SpaceError {
    OutOfMemory,
    AddressBusy,
    InvalidFormat,
    PagingError(paging::Error),
    Permission,
}

impl Into<solvent::Error> for SpaceError {
    fn into(self) -> solvent::Error {
        use solvent::*;
        Error(match self {
            SpaceError::OutOfMemory => ENOMEM,
            SpaceError::AddressBusy => EBUSY,
            SpaceError::InvalidFormat => EINVAL,
            SpaceError::PagingError(_) => EFAULT,
            SpaceError::Permission => EPERM,
        })
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
        task::Type::User => LAddr::from(minfo::USER_BASE)..LAddr::from(minfo::USER_TLS_BASE),
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

    tls: Mutex<Option<Layout>>,

    stack_blocks: Mutex<BTreeMap<LAddr, Layout>>,
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
            tls: Mutex::new(None),
            stack_blocks: Mutex::new(BTreeMap::new()),
        })
    }

    /// Allocate an address range in the space.
    pub fn allocate(
        self: &Arc<Self>,
        ty: AllocType,
        mut phys: Option<Arc<Phys>>,
        flags: Flags,
    ) -> Result<Virt, SpaceError> {
        self.canary.assert();
        let _pree = PREEMPT.lock();

        let ret = self
            .allocator
            .allocate(ty.clone(), &mut phys, flags, &self.arch);
        ret.map(|ptr| Virt::new(self.ty, ptr, phys.unwrap(), self.clone()))
    }

    /// Allocate an address range in the kernel space.
    ///
    /// Used for sharing kernel variables of [`KernelVirt`].
    pub fn allocate_kernel(
        self: &Arc<Self>,
        ty: AllocType,
        phys: Option<Arc<Phys>>,
        flags: Flags,
    ) -> Result<KernelVirt, SpaceError> {
        self.canary.assert();
        match self.ty {
            task::Type::Kernel => self
                .allocate(ty, phys, flags)
                .map(|virt| KernelVirt::new(virt).unwrap()),
            task::Type::User => Err(SpaceError::Permission),
        }
    }

    /// Modify the access flags of an address range without a specific type.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `ptr` was allocated by this `Space` and no
    /// pointers or references within the address range are present (or will be
    /// influenced by the modification).
    pub unsafe fn modify(&self, ptr: NonNull<[u8]>, flags: Flags) -> Result<(), SpaceError> {
        self.canary.assert();
        let _pree = PREEMPT.lock();

        self.allocator.modify(ptr, flags, &self.arch)
    }

    /// Deallocate an address range in the space without a specific type.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `ptr` was allocated by this `Space`.
    pub unsafe fn deallocate(&self, ptr: NonNull<u8>) -> Result<Arc<Phys>, SpaceError> {
        self.canary.assert();
        let _pree = PREEMPT.lock();

        self.allocator.deallocate(ptr, &self.arch)
    }

    /// # Safety
    ///
    /// The caller must ensure that loading the space is safe and not cause any
    /// #PF.
    pub unsafe fn load(&self) {
        self.canary.assert();
        self.arch.load()
    }

    pub fn alloc_tls<F, R>(
        &self,
        layout: Layout,
        init_func: F,
        realloc: bool,
    ) -> Result<Option<R>, SpaceError>
    where
        F: FnOnce(LAddr) -> R,
    {
        let _pree = PREEMPT.lock();
        if realloc {
            self.dealloc_tls();
        }

        let base = LAddr::from(minfo::USER_TLS_BASE);
        let mut tls = self.tls.lock();

        let ret = if tls.is_none() {
            let layout = layout
                .align_to(paging::PAGE_SIZE)
                .map_err(|_| SpaceError::InvalidFormat)?
                .pad_to_align();
            let size = layout.size();

            if minfo::USER_TLS_BASE + size > minfo::USER_TLS_END {
                return Err(SpaceError::OutOfMemory);
            }

            let alloc_ptr = unsafe { mem_alloc(layout) };
            if alloc_ptr.is_null() {
                return Err(SpaceError::OutOfMemory);
            }

            let virt = base..LAddr::from(base.val() + size);
            let phys = LAddr::new(alloc_ptr).to_paddr(minfo::ID_OFFSET);
            self.arch
                .maps(
                    virt,
                    phys,
                    Flags::READABLE | Flags::WRITABLE | Flags::EXECUTABLE | Flags::USER_ACCESS,
                )
                .map_err(|e| {
                    unsafe { mem_dealloc(alloc_ptr, layout) };
                    SpaceError::PagingError(e)
                })?;

            *tls = Some(layout);
            Some(init_func(base))
        } else {
            None
        };
        Ok(ret)
    }

    pub fn dealloc_tls(&self) {
        let _pree = PREEMPT.lock();
        if let Some(layout) = self.tls.lock().take() {
            let base = LAddr::from(minfo::USER_TLS_BASE);
            let virt = base..LAddr::from(base.val() + layout.size());

            if let Ok(Some(phys)) = self.arch.unmaps(virt) {
                let alloc_ptr = *phys.to_laddr(minfo::ID_OFFSET);
                unsafe { mem_dealloc(alloc_ptr, layout) };
            }
        }
    }

    fn alloc_stack(
        ty: task::Type,
        arch: &ArchSpace,
        stack_blocks: &mut MutexGuard<BTreeMap<LAddr, Layout>>,
        base: LAddr,
        size: usize,
    ) -> Result<LAddr, SpaceError> {
        let layout = {
            let n = size.div_ceil_bit(paging::PAGE_SHIFT);
            paging::PAGE_LAYOUT
                .repeat(n)
                .expect("Failed to get layout")
                .0
        };

        if base.val() < minfo::USER_STACK_BASE {
            return Err(SpaceError::OutOfMemory);
        }

        match ty {
            task::Type::User => {
                let (phys, alloc_ptr) = unsafe {
                    let ptr = mem_alloc(layout);

                    if ptr.is_null() {
                        return Err(SpaceError::OutOfMemory);
                    }

                    (LAddr::new(ptr).to_paddr(minfo::ID_OFFSET), ptr)
                };
                let virt = base..LAddr::from(base.val() + size);

                arch.maps(
                    virt,
                    phys,
                    Flags::READABLE | Flags::WRITABLE | Flags::USER_ACCESS,
                )
                .map_err(|e| unsafe {
                    mem_dealloc(alloc_ptr, layout);
                    SpaceError::PagingError(e)
                })?;

                if let Some(_) = stack_blocks.insert(base, layout) {
                    panic!("Duplicate allocation");
                }

                Ok(base)
            }
            task::Type::Kernel => {
                let ptr = unsafe { mem_alloc(layout) };
                Ok(LAddr::new(ptr))
            }
        }
    }

    pub fn init_stack(&self, size: usize) -> Result<LAddr, SpaceError> {
        self.canary.assert();
        let _pree = PREEMPT.lock();
        // if matches!(self.ty, task::Type::Kernel) {
        //       return Err("Stack allocation is not allowed in kernel");
        // }

        let size = size.round_up_bit(paging::PAGE_SHIFT);

        let base = Self::alloc_stack(
            self.ty,
            &self.arch,
            &mut self.stack_blocks.lock(),
            LAddr::from(minfo::USER_END - size),
            size,
        )?;

        Ok(LAddr::from(base.val() + size))
    }

    pub fn grow_stack(&self, addr: LAddr) -> Result<(), SpaceError> {
        self.canary.assert();
        let _pree = PREEMPT.lock();
        if matches!(self.ty, task::Type::Kernel) {
            return Err(SpaceError::Permission);
        }

        let addr = LAddr::from(addr.val().round_down_bit(paging::PAGE_SHIFT));

        let mut stack_blocks = self.stack_blocks.lock();

        let last = stack_blocks
            .iter()
            .next()
            .map_or(LAddr::from(minfo::USER_END), |(&k, _v)| k);

        let size = unsafe { last.offset_from(*addr) } as usize;

        Self::alloc_stack(self.ty, &self.arch, &mut stack_blocks, addr, size)?;

        Ok(())
    }

    pub fn clear_stack(&self) -> Result<(), SpaceError> {
        self.canary.assert();
        let _pree = PREEMPT.lock();

        let mut stack_blocks = self.stack_blocks.lock();
        while let Some((base, layout)) = stack_blocks.pop_first() {
            match self.ty {
                task::Type::Kernel => unsafe { mem_dealloc(*base, layout) },
                task::Type::User => {
                    let virt = base..LAddr::from(base.val() + layout.pad_to_align().size());
                    if let Ok(Some(phys)) = self.arch.unmaps(virt) {
                        let ptr = phys.to_laddr(minfo::ID_OFFSET);
                        unsafe { mem_dealloc(*ptr, layout) };
                    }
                }
            }
        }
        Ok(())
    }

    pub fn clone(this: &Arc<Self>, ty: task::Type) -> Arc<Self> {
        let ty = match this.ty {
            task::Type::Kernel => ty,
            task::Type::User => task::Type::User,
        };

        let _pree = PREEMPT.lock();
        Arc::new(Space {
            canary: Canary::new(),
            ty,
            arch: ArchSpace::clone(&this.arch),
            allocator: Arc::clone(&this.allocator),
            // TODO: Add an image field to `TaskInfo` to get TLS init block.
            tls: Mutex::new(None),
            stack_blocks: Mutex::new(BTreeMap::new()),
        })
    }
}

impl Drop for Space {
    fn drop(&mut self) {
        let _pree = PREEMPT.lock();
        let _ = self.clear_stack();
        self.dealloc_tls();
        unsafe { self.allocator.dispose(&self.arch) };
    }
}

/// Initialize the kernel memory space.
///
/// # Safety
///
/// The function must be called only once from the bootstrap CPU.
pub unsafe fn init_bsp_early() {
    KRL.load();
}

/// Load the kernel space for enery CPU.
///
/// # Safety
///
/// The function must be called only once from each application CPU.
pub unsafe fn init() {
    let space = KRL.clone();
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

/// Set the current memory space of the current CPU.
///
/// # Safety
///
/// The function must be called only from the epilogue of context switching.
pub unsafe fn set_current(space: Arc<Space>) {
    let _pree = PREEMPT.lock();
    space.load();
    CURRENT = Some(space);
}

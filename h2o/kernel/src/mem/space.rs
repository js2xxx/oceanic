//! # Address space management for H2O.
//!
//! This module is responsible for managing system memory and address space in a higher
//! level, especially for large objects like APIC.

use crate::sched::task;
use alloc::collections::BTreeMap;
use bitop_ex::BitOpEx;
use canary::Canary;
use collection_ex::RangeSet;
use paging::{LAddr, PAddr};

use core::alloc::Layout;
use core::mem::{align_of, size_of, MaybeUninit};
use core::ops::Range;
use core::pin::Pin;
use spin::{Mutex, MutexGuard};

cfg_if::cfg_if! {
      if #[cfg(target_arch = "x86_64")] {
            mod x86_64;
            type ArchSpace = x86_64::Space;
            pub use x86_64::MemBlock;
            pub use x86_64::init_pgc;
      }
}

static mut KRL_SPACE: Option<Space> = None;
#[thread_local]
static mut AP_SPACE: Option<Space> = None;

bitflags::bitflags! {
      /// Flags to describe a block of memory.
      pub struct Flags: u32 {
            const USER_ACCESS = 1;
            const READABLE    = 1 << 1;
            const WRITABLE    = 1 << 2;
            const EXECUTABLE  = 1 << 3;
            const ZEROED      = 1 << 4;
      }
}

/// The total available range of address space for the create type.
///
/// We cannot simply pass a [`Range`] to [`Space`]'s constructor because without control
/// arbitrary, even incanonical ranges would be passed and cause unrecoverable errors.
fn ty_to_range(ty: task::Type) -> Range<LAddr> {
      match ty {
            task::Type::Kernel => minfo::KERNEL_ALLOCABLE_RANGE,
            task::Type::User => LAddr::from(minfo::USER_BASE)..LAddr::from(minfo::USER_STACK_BASE),
      }
}

/// The structure that represents an address space.
///
/// The address space is defined from the concept of the virtual addressing in CPU. It's arch-
/// specific responsibility to map the virtual address to the real (physical) address in RAM.
/// This structure is used to allocate & reserve address space ranges for various requests.
///
/// >TODO: Support the requests for reserving address ranges.
#[derive(Debug)]
pub struct Space {
      canary: Canary<Space>,
      ty: task::Type,

      /// The arch-specific part of the address space.
      arch: ArchSpace,

      /// The free ranges in allocation.
      free_range: Mutex<RangeSet<LAddr>>,

      stack_blocks: Mutex<BTreeMap<LAddr, Layout>>,
}

unsafe impl Send for Space {}

impl Space {
      /// Create a new address space.
      pub fn new(ty: task::Type) -> Self {
            let mut free_range = RangeSet::new();
            let _ = free_range.insert(ty_to_range(ty));

            Space {
                  canary: Canary::new(),
                  ty,
                  arch: ArchSpace::new(),
                  free_range: Mutex::new(free_range),
                  stack_blocks: Mutex::new(BTreeMap::new()),
            }
      }

      /// Allocate an address range in the space without a specific type.
      ///
      /// # Safety
      ///
      /// The caller must ensure that the physical address is aligned with `layout`.
      pub unsafe fn alloc_manual(
            &self,
            layout: Layout,
            phys: Option<PAddr>,
            flags: Flags,
      ) -> Result<Pin<&mut [MemBlock]>, &'static str> {
            self.canary.assert();

            // Calculate the real size used.
            let layout = layout.align_to(align_of::<MemBlock>()).unwrap();
            let size = layout.pad_to_align().size();

            // Get the physical address mapped to.
            let (phys, alloc_ptr) = match phys {
                  Some(phys) => (phys, None),
                  None => {
                        let ptr = if flags.contains(Flags::ZEROED) {
                              alloc::alloc::alloc_zeroed(layout)
                        } else {
                              alloc::alloc::alloc(layout)
                        };

                        if ptr.is_null() {
                              return Err("Memory allocation failed");
                        }

                        (LAddr::new(ptr).to_paddr(minfo::ID_OFFSET), Some(ptr))
                  }
            };

            // Get the virtual address.
            // `prefix` and `suffix` are the gaps beside the allocated address range.
            let mut range = self.free_range.lock();
            let (prefix, virt, suffix) = {
                  let res = range.range_iter().find_map(|r| {
                        let mut start = r.start.val();
                        while start & (layout.align() - 1) != 0 {
                              start += 1 << start.trailing_zeros();
                        }
                        if start + size <= r.end.val() {
                              Some((
                                    r.start..LAddr::from(start),
                                    LAddr::from(start)..LAddr::from(start + size),
                                    LAddr::from(start + size)..r.end,
                              ))
                        } else {
                              None
                        }
                  });
                  match res {
                        Some((prefix, free, suffix)) => {
                              range.remove(prefix.start);
                              if !prefix.is_empty() {
                                    let _ = range.insert(prefix.clone());
                              }
                              if !suffix.is_empty() {
                                    let _ = range.insert(suffix.clone());
                              }
                              (prefix, free, suffix)
                        }
                        None => {
                              if let Some(alloc_ptr) = alloc_ptr {
                                    alloc::alloc::dealloc(alloc_ptr, layout);
                              }
                              return Err("No satisfactory virtual space");
                        }
                  }
            };

            // Map it.
            let ptr = *virt.start;
            self.arch.maps(virt, phys, flags).map_err(|_| {
                  if !prefix.is_empty() {
                        range.remove(prefix.start);
                  }
                  if !suffix.is_empty() {
                        range.remove(suffix.start);
                  }
                  let _ = range.insert(prefix.start..suffix.end);

                  if let Some(alloc_ptr) = alloc_ptr {
                        alloc::alloc::dealloc(alloc_ptr, layout);
                  }
                  "Paging error"
            })?;
            drop(range);

            // Build a memory block from the address range.
            let blocks = Pin::new_unchecked(core::slice::from_raw_parts_mut(
                  ptr.cast(),
                  size / size_of::<MemBlock>(),
            ));

            Ok(blocks)
      }

      /// Allocate an address range in the space with a specific type.
      ///
      /// # Safety
      ///
      /// The caller must ensure that the physical address is aligned with `layout`.
      pub unsafe fn alloc_typed<T>(
            &self,
            phys: Option<PAddr>,
            flags: Flags,
      ) -> Result<Pin<&mut MaybeUninit<T>>, &'static str> {
            self.canary.assert();

            self.alloc_manual(Layout::new::<T>(), phys, flags)
                  .and_then(MemBlock::into_typed)
      }

      /// Modify the access flags of an address range without a specific type.
      ///
      /// # Safety
      ///
      /// The caller must ensure that `b` was allocated by this `Space` and no pointers or
      /// references to the block are present (or influenced by the modification).
      pub unsafe fn modify_manual<'b>(
            &self,
            b: Pin<&'b mut [MemBlock]>,
            flags: Flags,
      ) -> Result<Pin<&'b mut [MemBlock]>, &'static str> {
            self.canary.assert();

            let virt = {
                  let ptr = b.as_ptr_range();
                  LAddr::new(ptr.start as *mut _)..LAddr::new(ptr.end as *mut _)
            };

            self.arch
                  .reprotect(virt, flags)
                  .map_err(|_| "Paging error")?;

            Ok(b)
      }

      /// Modify the access flags of an address range with a specific type.
      ///
      /// # Safety
      ///
      /// The caller must ensure that `b` was allocated by this `Space` and no pointers or
      /// references to the block are present (or influenced by the modification).
      pub unsafe fn modify_typed<'b, T>(
            &self,
            b: Pin<&'b mut MaybeUninit<T>>,
            flags: Flags,
      ) -> Result<Pin<&'b mut MaybeUninit<T>>, &'static str> {
            self.canary.assert();

            self.modify_manual(MemBlock::from_typed(b), flags)
                  .and_then(MemBlock::into_typed)
      }

      /// Deallocate an address range in the space without a specific type.
      ///
      /// # Safety
      ///
      /// The caller must ensure that `b` was allocated by this `Space` and `free_phys`
      /// is only set if the physical address range is allocated within `b`'s allocation.
      pub unsafe fn dealloc_manual(
            &self,
            b: Pin<&mut [MemBlock]>,
            free_phys: bool,
      ) -> Result<(), &'static str> {
            self.canary.assert();

            // Get the virtual address range from the given memory block.
            let layout = Layout::for_value(&*b);
            let mut virt = {
                  let ptr = b.as_ptr_range();
                  LAddr::new(ptr.start as *mut _)..LAddr::new(ptr.end as *mut _)
            };

            // Unmap the virtual address & get the physical address.
            let phys = self.arch.unmaps(virt.clone()).map_err(|_| "Paging error")?;
            if free_phys {
                  if let Some(phys) = phys {
                        let alloc_ptr = phys.to_laddr(minfo::ID_OFFSET);
                        alloc::alloc::dealloc(*alloc_ptr, layout);
                  }
            }

            // Deallocate the virtual address range.
            let mut range = self.free_range.lock();
            let (prefix, suffix) = range.neighbors(virt.clone());
            if let Some(prefix) = prefix {
                  virt.start = prefix.start;
                  range.remove(prefix.start);
            }
            if let Some(suffix) = suffix {
                  virt.end = suffix.end;
                  range.remove(suffix.start);
            }
            range.insert(virt).map_err(|_| "Occupied range")
      }

      /// Deallocate an address range in the space without a specific type.
      ///
      /// # Safety
      ///
      /// The caller must ensure that `b` was allocated by this `Space` and `free_phys`
      /// is only set if the physical address range is allocated within `b`'s allocation.
      pub unsafe fn dealloc_typed<T>(
            &self,
            b: Pin<&mut MaybeUninit<T>>,
            free_phys: bool,
      ) -> Result<(), &'static str> {
            self.canary.assert();

            self.dealloc_manual(MemBlock::from_typed(b), free_phys)
      }

      /// # Safety
      ///
      /// The caller must ensure that loading the space is safe and not cause any #PF.
      pub unsafe fn load(&self) {
            self.canary.assert();
            self.arch.load()
      }

      fn alloc_stack(
            arch: &ArchSpace,
            stack_blocks: &mut MutexGuard<BTreeMap<LAddr, Layout>>,
            base: LAddr,
            size: usize,
      ) -> Result<(), &'static str> {
            let layout = {
                  let n = size.div_ceil_bit(paging::PAGE_SHIFT);
                  paging::PAGE_LAYOUT
                        .repeat(n)
                        .expect("Failed to get layout")
                        .0
            };

            if base.val() < minfo::USER_STACK_BASE {
                  return Err("Max allocation size exceeded");
            }

            let (phys, alloc_ptr) = unsafe {
                  let ptr = alloc::alloc::alloc(layout);

                  if ptr.is_null() {
                        return Err("Memory allocation failed");
                  }

                  (LAddr::new(ptr).to_paddr(minfo::ID_OFFSET), ptr)
            };
            let virt = base..LAddr::from(base.val() + size);

            arch.maps(virt, phys, Flags::READABLE | Flags::WRITABLE)
                  .map_err(|_| unsafe {
                        alloc::alloc::dealloc(alloc_ptr, layout);
                        "Paging error"
                  })?;

            if let Some(_) = stack_blocks.insert(base, layout) {
                  panic!("Duplicate allocation");
            }

            Ok(())
      }

      pub fn init_stack(&self, size: usize) -> Result<LAddr, &'static str> {
            self.canary.assert();
            // if matches!(self.ty, task::Type::Kernel) {
            //       return Err("Stack allocation is not allowed in kernel");
            // }

            let size = size.round_up_bit(paging::PAGE_SHIFT);

            let top = minfo::USER_END;
            let base = LAddr::from(top - size);

            Self::alloc_stack(&self.arch, &mut self.stack_blocks.lock(), base, size)?;

            Ok(LAddr::from(top))
      }

      pub fn grow_stack(&self, addr: LAddr) -> Result<(), &'static str> {
            self.canary.assert();
            // if matches!(self.ty, task::Type::Kernel) {
            //       return Err("Stack allocation is not allowed in kernel");
            // }

            let addr = LAddr::from(addr.val().round_down_bit(paging::PAGE_SHIFT));

            let mut stack_blocks = self.stack_blocks.lock();

            let last = stack_blocks
                  .iter()
                  .next()
                  .map_or(LAddr::from(minfo::USER_END), |(&k, _v)| k);

            let size = unsafe { last.offset_from(*addr) } as usize;

            Self::alloc_stack(&self.arch, &mut stack_blocks, addr, size)
      }

      pub fn clear_stack(&self) -> Result<(), &'static str> {
            self.canary.assert();
            // if matches!(self.ty, task::Type::Kernel) {
            //       return Err("Stack allocation is not allowed in kernel");
            // }

            let mut stack_blocks = self.stack_blocks.lock();
            for (&base, &layout) in stack_blocks.iter() {
                  let virt = base..LAddr::from(base.val() + layout.pad_to_align().size());
                  if let Ok(Some(phys)) = self.arch.unmaps(virt) {
                        let ptr = phys.to_laddr(minfo::ID_OFFSET);
                        unsafe { alloc::alloc::dealloc(*ptr, layout) };
                  }
            }

            stack_blocks.clear();
            Ok(())
      }
}

/// Initialize the kernel space for the bootstrap CPU.
///
/// # Safety
///
/// The function must be called only once from the bootstrap CPU.
pub unsafe fn init_kernel() {
      let krl_space = Space::new(task::Type::Kernel);
      krl_space.load();
      KRL_SPACE = Some(krl_space);
}

/// Initialize the kernel space for the application CPU.
///
/// # Safety
///
/// The function must be called only once from each application CPU.
pub unsafe fn init_ap() {
      let ap_space = Space::new(task::Type::Kernel);
      ap_space.load();
      AP_SPACE = Some(ap_space);
}

/// Get the reference of the per-CPU kernel space.
pub fn krl<F, R>(f: F) -> Option<R>
where
      F: FnOnce(&'static Space) -> R,
{
      let k = unsafe {
            if crate::cpu::id() == 0 {
                  KRL_SPACE.as_ref()
            } else {
                  AP_SPACE.as_ref()
            }
      };
      k.map(|krl| f(krl))
}

/// # Safety
///
/// The caller must ensure that the current loaded space is the kernel space.
pub unsafe fn with<'s, F, R>(space: &'s Space, f: F) -> Option<R>
where
      F: FnOnce(&'s Space) -> R,
{
      krl(|krl| {
            space.load();
            let ret = f(space);
            krl.load();

            ret
      })
}

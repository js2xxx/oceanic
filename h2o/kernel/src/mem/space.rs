//! # Address space management for H2O.
//!
//! This module is responsible for managing system memory and address space in a higher
//! level, especially for large objects like GDT and APIC.

use canary::Canary;
use collection_ex::RangeSet;
use paging::{LAddr, PAddr};

use alloc::sync::Arc;
use core::alloc::Layout;
use core::mem::{align_of, size_of, MaybeUninit};
use core::ops::Range;
use core::pin::Pin;
use spin::Mutex;

cfg_if::cfg_if! {
      if #[cfg(target_arch = "x86_64")] {
            mod x86_64;
            type ArchSpace = x86_64::Space;
            pub use x86_64::MemBlock;
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

/// The create type of a [`Space`].
#[derive(Debug)]
pub enum CreateType {
      Kernel,
      User,
}

impl CreateType {
      /// The total available range of address space for the create type.
      ///
      /// We cannot simply pass a [`Range`] to [`Space`]'s constructor because without control
      /// arbitrary, even incanonical ranges would be passed and cause unrecoverable errors.
      fn range(&self) -> Range<LAddr> {
            match self {
                  CreateType::Kernel => minfo::KERNEL_ALLOCABLE_RANGE,
                  CreateType::User => LAddr::from(minfo::USER_BASE)..LAddr::from(minfo::USER_END),
            }
      }
}

/// The structure that represents an address space.
///
/// The address space is defined from the concept of the virtual addressing in CPU. It's arch-
/// specific responsibility to map the virtual address to the real (physical) address in RAM.
/// This structure is used to allocate & reserve address space ranges for various requests.
///
/// >TODO: Support the requests for reserving address ranges.
pub struct Space {
      canary: Canary<Space>,

      /// The arch-specific part of the address space.
      arch: Arc<ArchSpace>,

      /// The free ranges in allocation.
      free_range: Mutex<RangeSet<LAddr>>,
}

impl Space {
      /// Create a new address space.
      pub fn new(ty: CreateType) -> Self {
            let mut free_range = RangeSet::new();
            let _ = free_range.insert(ty.range());

            Space {
                  canary: Canary::new(),
                  arch: ArchSpace::new(),
                  free_range: Mutex::new(free_range),
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
                  .and_then(|r| MemBlock::into_typed(r))
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
            self.arch.load()
      }
}

/// Initialize the kernel space for the bootstrap CPU.
///
/// # Safety
///
/// The function must be called only once from the bootstrap CPU.
pub unsafe fn init_kernel() {
      let krl_space = Space::new(CreateType::Kernel);
      krl_space.load();
      KRL_SPACE.insert(krl_space);
}

/// Get the reference of the per-CPU kernel space.
///
/// # Safety
///
/// The function must be called only after the CPU's kernel space is initialized and loaded.
pub unsafe fn krl<F, R>(f: F) -> Option<R>
where
      F: FnOnce(&'static Space) -> R,
{
      let k = if crate::cpu::id() == 0 {
            KRL_SPACE.as_ref()
      } else {
            AP_SPACE.as_ref()
      };
      k.map(|krl| f(krl))
}

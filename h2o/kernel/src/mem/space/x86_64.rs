//! # Memory management of H2O in x86_64.
//!
//! This module is specific for x86_64 mode. It wraps the cr3's root page table and the methods
//! of x86_64 paging.

use super::Flags;
use bitop_ex::BitOpEx;
use canary::Canary;
use paging::{LAddr, PAddr, Table};

use alloc::boxed::Box;
use core::alloc::Layout;
use core::mem::{align_of, size_of, MaybeUninit};
use core::ops::Range;
use core::pin::Pin;
use spin::{Lazy, Mutex};

/// The root page table at initialization time.
static KERNEL_ROOT: Lazy<(Box<Table>, u64)> = Lazy::new(|| {
      let mut table = box Table::zeroed();

      let cr3 = unsafe { archop::reg::cr3::read() };
      let cr3_laddr = PAddr::new(cr3 as usize).to_laddr(minfo::ID_OFFSET);
      let init_table = unsafe { core::slice::from_raw_parts(cr3_laddr.cast(), paging::NR_ENTRIES) };
      table.copy_from_slice(init_table);

      (table, cr3)
});

pub fn init_pgc() -> u64 {
      KERNEL_ROOT.1
}

/// The root page table.
#[derive(Debug)]
pub struct Space {
      canary: Canary<Space>,
      root_table: Mutex<Box<Table>>,
      cr3: PAddr,
}

impl Space {
      /// Construct a new arch-specific space.
      ///
      /// The space's root page table must contains the page tables of the kernel half otherwise
      /// if loaded the kernel will crash due to #PF.
      pub fn new() -> Space {
            let rt = box Table::zeroed();
            let cr3 = Box::into_raw(rt);

            let space = Space {
                  canary: Canary::new(),
                  root_table: Mutex::new(unsafe { Box::from_raw(cr3) }),
                  cr3: LAddr::new(cr3.cast()).to_paddr(minfo::ID_OFFSET),
            };

            {
                  // So far we only copy the higher half kernel mappings. In the future, we'll set
                  // ranges and customize the behavior.
                  let mut dst_rt = space.root_table.lock();
                  let dst_kernel_half = &mut dst_rt[(paging::NR_ENTRIES / 2)..];

                  let src_kernel_half = &KERNEL_ROOT.0[(paging::NR_ENTRIES / 2)..];

                  dst_kernel_half.copy_from_slice(src_kernel_half);
            }

            space
      }

      pub(in crate::mem) fn maps(
            &self,
            virt: Range<LAddr>,
            phys: PAddr,
            flags: Flags,
      ) -> Result<(), paging::Error> {
            self.canary.assert();

            let attr = paging::Attr::builder()
                  .writable(flags.contains(Flags::WRITABLE))
                  .user_access(flags.contains(Flags::USER_ACCESS))
                  .executable(flags.contains(Flags::EXECUTABLE))
                  .build();

            let map_info = paging::MapInfo {
                  virt,
                  phys,
                  attr,
                  id_off: minfo::ID_OFFSET,
            };

            paging::maps(&mut *self.root_table.lock(), &map_info, &mut PageAlloc)
      }

      pub(in crate::mem) fn reprotect(
            &self,
            virt: Range<LAddr>,
            flags: Flags,
      ) -> Result<(), paging::Error> {
            self.canary.assert();

            let attr = paging::Attr::builder()
                  .writable(flags.contains(Flags::WRITABLE))
                  .user_access(flags.contains(Flags::USER_ACCESS))
                  .executable(flags.contains(Flags::EXECUTABLE))
                  .build();

            let reprotect_info = paging::ReprotectInfo {
                  virt,
                  attr,
                  id_off: minfo::ID_OFFSET,
            };

            paging::reprotect(
                  &mut *self.root_table.lock(),
                  &reprotect_info,
                  &mut PageAlloc,
            )
      }

      // pub fn query(&self, virt: LAddr) -> Result<PAddr, paging::Error> {
      //       self.canary.assert();

      //       paging::query(&mut *self.root_table.lock(), virt, minfo::ID_OFFSET)
      // }

      pub(in crate::mem) fn unmaps(
            &self,
            virt: Range<LAddr>,
      ) -> Result<Option<PAddr>, paging::Error> {
            self.canary.assert();

            let mut lck = self.root_table.lock();
            let phys = paging::query(&mut lck, virt.start, minfo::ID_OFFSET).ok();
            paging::unmaps(&mut lck, virt, minfo::ID_OFFSET, &mut PageAlloc).map(|_| phys)
      }

      /// # Safety
      ///
      /// The caller must ensure that loading the space is safe and not cause any #PF.
      pub(in crate::mem) unsafe fn load(&self) {
            archop::reg::cr3::write(*self.cr3 as u64);
      }
}

impl Clone for Space {
      fn clone(&self) -> Self {
            let rt = box Table::zeroed();
            let cr3 = Box::into_raw(rt);
            let mut rt = unsafe { Box::from_raw(cr3) };

            rt.copy_from_slice(&self.root_table.lock()[..]);
            // TODO: Set up the page-fault handler for task cloning.
            let level = paging::Level::P4;
            for ent in rt.iter_mut().take(paging::NR_ENTRIES / 2) {
                  let (phys, attr) = (*ent).get(level);
                  *ent = paging::Entry::new(phys, attr & !paging::Attr::PRESENT, level);
            }

            Space {
                  canary: Canary::new(),
                  root_table: Mutex::new(rt),
                  cr3: LAddr::new(cr3.cast()).to_paddr(minfo::ID_OFFSET),
            }
      }
}

/// The standard memory block representing a page in x86_64 mode.
///
/// This structure is only used for allocating unknown types and its data can only be accessed
/// via type conversion.
#[repr(align(4096))]
pub struct MemBlock {
      _data: [u8; 4096],
}

impl MemBlock {
      pub fn into_typed<'a, T>(
            blocks: Pin<&'a mut [Self]>,
      ) -> Result<Pin<&'a mut MaybeUninit<T>>, &'static str> {
            if blocks.len() * size_of::<MemBlock>() < size_of::<T>() {
                  Err("The size is not satisfied")
            } else {
                  Ok(unsafe { blocks.map_unchecked_mut(|u| &mut *u.as_mut_ptr().cast()) })
            }
      }

      /// # Safety
      ///
      /// The caller must ensure that the block is allocated by [`super::Space`].
      pub unsafe fn from_typed<'a, T>(b: Pin<&'a mut MaybeUninit<T>>) -> Pin<&'a mut [Self]> {
            b.map_unchecked_mut(|u| {
                  core::slice::from_raw_parts_mut(
                        (u as *mut MaybeUninit<T>).cast(),
                        Layout::new::<T>()
                              .align_to(align_of::<MemBlock>())
                              .unwrap()
                              .pad_to_align()
                              .size()
                              .div_ceil_bit(paging::PAGE_SHIFT),
                  )
            })
      }

      // pub fn into_raw<'a>(b: Pin<&'a mut [Self]>) -> (LAddr, usize) {
      //       (LAddr::new(b.as_ptr() as *mut u8), b.len())
      // }

      // /// # Safety
      // ///
      // /// The caller must ensure that the data is acquired from [`MemBlock::into_raw`].
      // pub unsafe fn from_raw<'a>(addr: LAddr, len: usize) -> Pin<&'a mut [Self]> {
      //       Pin::new_unchecked(core::slice::from_raw_parts_mut(addr.cast(), len))
      // }
}

struct PageAlloc;

unsafe impl paging::PageAlloc for PageAlloc {
      unsafe fn alloc(&mut self) -> Option<PAddr> {
            let ptr = alloc::alloc::alloc(core::alloc::Layout::new::<paging::Table>());
            if !ptr.is_null() {
                  Some(LAddr::new(ptr).to_paddr(minfo::ID_OFFSET))
            } else {
                  None
            }
      }

      unsafe fn dealloc(&mut self, addr: PAddr) {
            let ptr = *addr.to_laddr(minfo::ID_OFFSET);
            alloc::alloc::dealloc(ptr, core::alloc::Layout::new::<paging::Table>());
      }
}

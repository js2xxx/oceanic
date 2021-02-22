use crate::mem::extent;
use canary::Canary;
use paging::{LAddr, PAddr, Table};

use alloc::boxed::Box;
use core::ops::Range;
use lazy_static::lazy_static;
use spin::Mutex;

lazy_static! {
      static ref KERNEL_SPACE: Space = {
            let space = Space {
                  canary: Canary::new(),
                  root_table: Mutex::new(box Table::zeroed()),
            };
            let cr3_laddr = PAddr::new(unsafe { archop::reg::cr3::read() } as usize)
                  .to_laddr(minfo::ID_OFFSET);
            let init_table =
                  unsafe { core::slice::from_raw_parts(cr3_laddr.cast(), paging::NR_ENTRIES) };
            space.root_table.lock().copy_from_slice(init_table);
            space
      };
}

pub struct Space {
      canary: Canary<Space>,
      root_table: Mutex<Box<Table>>,
}

impl Space {
      pub fn new() -> Space {
            let space = Space {
                  canary: Canary::new(),
                  root_table: Mutex::new(box Table::zeroed()),
            };

            {
                  // So far we only copy the higher half kernel mappings. In the future, we'll set ranges
                  // and customize the behavior.
                  let mut dst_rt = space.root_table.lock();
                  let dst_kernel_half = dst_rt.split_at_mut(paging::NR_ENTRIES / 2).1;

                  let mut src_rt = KERNEL_SPACE.root_table.lock();
                  let src_kernel_half = src_rt.split_at(paging::NR_ENTRIES / 2).1;

                  dst_kernel_half.copy_from_slice(src_kernel_half);
            }
            
            space
      }

      pub fn maps(
            &self,
            virt: Range<LAddr>,
            phys: PAddr,
            flags: extent::Flags,
      ) -> Result<(), paging::Error> {
            self.canary.assert();

            let attr = paging::Attr::builder()
                  .writable(flags.contains(extent::Flags::WRTIEABLE))
                  .user_access(flags.contains(extent::Flags::USER_ACCESS))
                  .executable(flags.contains(extent::Flags::EXECUTABLE))
                  .build();

            let map_info = paging::MapInfo {
                  virt,
                  phys,
                  attr,
                  id_off: minfo::ID_OFFSET,
            };

            paging::maps(&mut *self.root_table.lock(), &map_info, &mut PageAlloc)
      }

      pub fn query(&self, virt: LAddr) -> Result<PAddr, paging::Error> {
            self.canary.assert();

            paging::query(&mut *self.root_table.lock(), virt, minfo::ID_OFFSET)
      }

      pub fn unmaps(&self, virt: Range<LAddr>) -> Result<(), paging::Error> {
            self.canary.assert();

            paging::unmaps(
                  &mut *self.root_table.lock(),
                  virt,
                  minfo::ID_OFFSET,
                  &mut PageAlloc,
            )
      }
}

struct PageAlloc;

unsafe impl paging::PageAlloc for PageAlloc {
      unsafe fn alloc(&mut self) -> Option<PAddr> {
            let ptr = alloc::alloc::alloc(core::alloc::Layout::new::<paging::Table>());
            let paddr = LAddr::new(ptr).to_paddr(minfo::ID_OFFSET);
            if *paddr != 0 {
                  Some(paddr)
            } else {
                  None
            }
      }

      unsafe fn dealloc(&mut self, addr: PAddr) {
            let ptr = *addr.to_laddr(minfo::ID_OFFSET);
            alloc::alloc::dealloc(ptr, core::alloc::Layout::new::<paging::Table>());
      }
}

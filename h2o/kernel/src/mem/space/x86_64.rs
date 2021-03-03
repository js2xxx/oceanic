use crate::mem::extent;
use canary::Canary;
use paging::{LAddr, PAddr, Table};

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::{Arc, Weak};
use core::ops::Range;
use lazy_static::lazy_static;
use spin::Mutex;

lazy_static! {
      static ref KERNEL_ROOT: Box<Table> = {
            let mut table = box Table::zeroed();

            let cr3_laddr = PAddr::new(unsafe { archop::reg::cr3::read() } as usize)
                  .to_laddr(minfo::ID_OFFSET);
            let init_table =
                  unsafe { core::slice::from_raw_parts(cr3_laddr.cast(), paging::NR_ENTRIES) };
            table.copy_from_slice(init_table);

            table
      };
}

#[derive(Debug)]
pub enum CreateType {
      Kernel,
      User,
}

impl CreateType {
      fn range(&self) -> Range<LAddr> {
            match self {
                  CreateType::Kernel => minfo::KERNEL_ALLOCABLE_RANGE,
                  CreateType::User => LAddr::from(minfo::USER_BASE)..LAddr::from(minfo::USER_END),
            }
      }
}

#[derive(Debug)]
pub struct Space {
      canary: Canary<Space>,
      extent: Arc<extent::Extent>,
      root_table: Mutex<Box<Table>>,
}

impl Space {
      pub fn new(ty: CreateType, flags: extent::Flags) -> Arc<Space> {
            let mut extent = Arc::new(extent::Extent::new(
                  Weak::new(),
                  ty.range(),
                  flags,
                  extent::Type::Region(BTreeMap::new()),
            ));

            let mut space = Arc::new(Space {
                  canary: Canary::new(),
                  extent: extent.clone(),
                  root_table: Mutex::new(box Table::zeroed()),
            });

            *extent.space.write() = Arc::downgrade(&space);

            {
                  // So far we only copy the higher half kernel mappings. In the future, we'll set
                  // ranges and customize the behavior.
                  let mut dst_rt = space.root_table.lock();
                  let dst_kernel_half = &mut dst_rt[(paging::NR_ENTRIES / 2)..];

                  let src_kernel_half = &KERNEL_ROOT[(paging::NR_ENTRIES / 2)..];

                  dst_kernel_half.copy_from_slice(src_kernel_half);
            }

            space
      }

      pub fn extent(&self) -> &Arc<extent::Extent> {
            &self.extent
      }

      pub(in crate::mem) fn maps(
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

      pub(in crate::mem) fn query(&self, virt: LAddr) -> Result<PAddr, paging::Error> {
            self.canary.assert();

            paging::query(&mut *self.root_table.lock(), virt, minfo::ID_OFFSET)
      }

      pub(in crate::mem) fn unmaps(&self, virt: Range<LAddr>) -> Result<(), paging::Error> {
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

#![no_std]
#![feature(asm)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(step_trait)]
#![feature(step_trait_ext)]

pub mod addr;
pub mod alloc;
pub mod entry;
pub mod level;

use core::ops::Range;
use core::ptr::NonNull;

pub use addr::{LAddr, PAddr};
pub use alloc::PageAlloc;
pub use entry::{Attr, Entry};
pub use level::Level;

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;

pub const ENTRY_SIZE_SHIFT: usize = 3;
pub const NR_ENTRIES_SHIFT: usize = PAGE_SHIFT - ENTRY_SIZE_SHIFT;
pub const NR_ENTRIES: usize = 1 << NR_ENTRIES_SHIFT;

pub const CANONICAL_PREFIX: usize = 0xFFFF_0000_0000_0000;

pub const RECURSIVE_IDX: usize = 510;

#[derive(Copy, Clone, Debug)]
pub enum Error {
      OutOfMemory,
      AddrMisaligned {
            vstart: Option<LAddr>,
            vend: Option<LAddr>,
            phys: Option<PAddr>,
      },
      RangeEmpty,
      EntryExistent(bool),
}

#[derive(Clone)]
pub struct MapInfo {
      pub virt: Range<LAddr>,
      pub phys: PAddr,
      pub attr: Attr,
      pub id_off: usize,
}

impl Error {
      fn is_misaligned_invalid(&self) -> bool {
            matches!(
                  *self,
                  Error::AddrMisaligned {
                        vstart: None,
                        vend: None,
                        phys: None,
                  }
            )
      }
}

impl MapInfo {
      fn advance(&mut self, offset: usize) {
            self.virt.start.advance(offset);
            self.phys = PAddr::new(*self.phys + offset);
      }

      fn distance(&self, other: &MapInfo) -> Range<LAddr> {
            self.virt.start..other.virt.start
      }
}

fn create_table<'a, 'b: 'a>(
      entry: &'b mut Entry,
      level: Level,
      id_off: usize,
      allocator: &'a mut impl PageAlloc,
) -> Result<NonNull<[Entry]>, Error> {
      assert!(level != Level::Pt, "Too low level");

      match entry.get_table(id_off, level) {
            Some(ptr) => Ok(ptr),
            None => {
                  if entry.is_leaf(level) {
                        Err(Error::EntryExistent(true))
                  } else {
                        let phys = allocator
                              .alloc_zeroed(id_off)
                              .map_or(Err(Error::OutOfMemory), Ok)?;
                        let attr = Attr::INTERMEDIATE;
                        *entry = Entry::new(phys, attr, level);
                        Ok(entry
                              .get_table(id_off, level)
                              .expect("Failed to get the table of the entry"))
                  }
            }
      }
}

unsafe fn invalidate_page(virt: LAddr) {
      asm!("invlpg [{}]", in(reg) *virt);
}

fn new_page(
      root_table: NonNull<[Entry]>,
      virt: LAddr,
      phys: PAddr,
      attr: Attr,
      level: Level,
      id_off: usize,
      allocator: &mut impl PageAlloc,
) -> Result<(), Error> {
      let mut table = root_table;
      for lvl in Level::P4..level {
            let idx = lvl.addr_idx(virt, false);
            let table_mut = unsafe { table.as_mut() };
            let item = &mut table_mut[idx];
            table = create_table(item, lvl, id_off, allocator)?;
      }

      let idx = level.addr_idx(virt, false);
      let table_mut = unsafe { table.as_mut() };

      if table_mut[idx].is_leaf(level) {
            Err(Error::EntryExistent(true))
      } else {
            let attr = level.leaf_attr(attr);
            table_mut[idx] = Entry::new(phys, attr, level);

            unsafe { invalidate_page(virt) };
            Ok(())
      }
}

fn check(virt: &Range<LAddr>, phys: Option<PAddr>) -> Result<(), Error> {
      #[inline]
      fn misaligned<Origin>(addr: usize, o: Origin) -> Option<Origin> {
            if addr & (PAGE_SIZE - 1) == 0 {
                  None
            } else {
                  Some(o)
            }
      }

      let (vstart, vend) = (virt.start.val(), virt.end.val());
      let ret = Error::AddrMisaligned {
            vstart: misaligned(vstart, virt.start),
            vend: misaligned(vend, virt.end),
            phys: phys.and_then(|phys| misaligned(*phys, phys)),
      };
      if !ret.is_misaligned_invalid() {
            return Err(ret);
      }

      if vstart >= vend {
            return Err(Error::RangeEmpty);
      }

      Ok(())
}

pub fn maps(
      root_table: NonNull<[Entry]>,
      info: &MapInfo,
      allocator: &mut impl PageAlloc,
) -> Result<(), Error> {
      check(&info.virt, Some(info.phys))?;

      let mut rem_info = info.clone();
      while !info.virt.is_empty() {
            let level = core::cmp::min(
                  Level::fit(info.virt.start.val()).expect("Misaligned start address"),
                  Level::fit(info.virt.end.val() - info.virt.start.val())
                        .expect("Misaligned start address"),
            );

            let ret = new_page(
                  root_table,
                  rem_info.virt.start,
                  rem_info.phys,
                  info.attr,
                  level,
                  info.id_off,
                  allocator,
            );
            if ret.is_err() {
                  let done_virt = info.distance(&rem_info);
                  let _ = unmaps(root_table, done_virt, info.id_off, allocator);
                  return ret;
            }

            let ps = level.page_size();
            rem_info.advance(ps);
      }

      Ok(())
}

fn split_table(
      entry: &mut Entry,
      level: Level,
      id_off: usize,
      allocator: &mut impl PageAlloc,
) -> Result<(), Error> {
      let (phys, mut attr) = entry.get(level);
      entry.reset();
      attr &= !Attr::LARGE_PAT;

      let item_level = level.decrease().expect("Too low level");
      let item_attr = item_level.leaf_attr(attr);
      let mut table = create_table(entry, level, id_off, allocator)?;
      let table_mut = unsafe { table.as_mut() };
      for (i, item) in table_mut.iter_mut().enumerate() {
            let item_phys = PAddr::new(*phys + i * item_level.page_size());
            *item = Entry::new(item_phys, item_attr, item_level);
      }

      Ok(())
}

fn get_or_split_table(
      entry: &mut Entry,
      level: Level,
      id_off: usize,
      allocator: &mut impl PageAlloc,
) -> Result<NonNull<[Entry]>, Error> {
      assert!(level != Level::Pt, "Too low level");

      match entry.get_table(id_off, level) {
            Some(ptr) => Ok(ptr),
            None => {
                  if entry.is_leaf(level) {
                        split_table(entry, level, id_off, allocator)?;

                        Ok(entry
                              .get_table(id_off, level)
                              .expect("Failed to get the table of the entry"))
                  } else {
                        Err(Error::EntryExistent(false))
                  }
            }
      }
}

fn drop_page(
      root_table: NonNull<[Entry]>,
      virt: LAddr,
      level: Level,
      id_off: usize,
      allocator: &mut impl PageAlloc,
) -> Result<(), Error> {
      let mut table = root_table;
      for lvl in Level::P4..level {
            let idx = lvl.addr_idx(virt, false);
            let table_mut = unsafe { table.as_mut() };
            let item = &mut table_mut[idx];
            table = get_or_split_table(item, lvl, id_off, allocator)?;
      }

      let idx = level.addr_idx(virt, false);
      let table_mut = unsafe { table.as_mut() };

      if table_mut[idx].is_leaf(level) {
            table_mut[idx].reset();

            unsafe { invalidate_page(virt) };
            Ok(())
      } else {
            Err(Error::EntryExistent(false))
      }
}

pub fn unmaps(
      root_table: NonNull<[Entry]>,
      mut virt: Range<LAddr>,
      id_off: usize,
      allocator: &mut impl PageAlloc,
) -> Result<(), Error> {
      check(&virt, None)?;

      while !virt.is_empty() {
            let level = core::cmp::min(
                  Level::fit(virt.start.val()).expect("Misaligned start address"),
                  Level::fit(virt.end.val() - virt.start.val()).expect("Misaligned start address"),
            );

            let _ = drop_page(root_table, virt.start, level, id_off, allocator);

            let ps = level.page_size();
            virt.start.advance(ps);
      }

      Ok(())
}

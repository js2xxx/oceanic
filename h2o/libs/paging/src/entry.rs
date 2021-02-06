use crate::{level::Level, NR_ENTRIES};
use crate::{PAddr, ENTRY_SIZE_SHIFT};

use bitflags::bitflags;
use core::num::NonZeroUsize;
use core::ptr::NonNull;
use static_assertions::*;

const LOCK_SHIFT: usize = 9;
const MUT_LOCK_SHIFT: usize = 10;

bitflags! {
      pub struct Attr: u64 {
            const PRESENT     = 1;
            const WRITABLE    = 1 << 1;
            const USER_ACCESS = 1 << 2;
            const WRITE_THRU  = 1 << 3;
            const CACHE_DISABLE = 1 << 4;
            const ACCESSED    = 1 << 5;
            const DIRTY       = 1 << 6;
            const LARGE_PAGE  = 1 << 7;
            const PAT         = Self::LARGE_PAGE.bits();
            const GLOBAL      = 1 << 8;
            const LOCKED      = 1 << LOCK_SHIFT;
            const MUT_LOCKED  = 1 << MUT_LOCK_SHIFT;
            const _UNUSED     = 1 << 11;
            const LARGE_PAT   = 1 << 12;
            const EXE_DISABLE = 1 << 63;

            const KERNEL_R    = Self::empty().bits;
            const KERNEL_RNE  = Self::KERNEL_R.bits    | Self::EXE_DISABLE.bits;
            const KERNEL_RW   = Self::KERNEL_R.bits    | Self::WRITABLE.bits;
            const KERNEL_RWNE = Self::KERNEL_RNE.bits  | Self::WRITABLE.bits;
            const USER_R      = Self::KERNEL_R.bits    | Self::USER_ACCESS.bits;
            const USER_RNE    = Self::KERNEL_RNE.bits  | Self::USER_ACCESS.bits;
            const USER_RW     = Self::KERNEL_RW.bits   | Self::USER_ACCESS.bits;
            const USER_RWNE   = Self::KERNEL_RWNE.bits | Self::USER_ACCESS.bits;

            const INTERMEDIATE = Self::USER_RW.bits;
      }
}

#[derive(Copy, Clone, Debug)]
pub struct Entry(u64);
const_assert!(core::mem::size_of::<Entry>() == 1 << ENTRY_SIZE_SHIFT);

impl Attr {
      pub fn merge(&mut self, other: &Attr) {
            *self |= *other & Self::USER_RW;
            *self &= !Self::ACCESSED;
            *self &= *other & Self::EXE_DISABLE;
      }

      #[inline]
      pub fn has_table(&self, level: Level) -> bool {
            !(level == Level::Pt || self.contains(Attr::LARGE_PAGE))
      }
}

impl From<Entry> for Attr {
      fn from(e: Entry) -> Self {
            Attr::from_bits_truncate(e.0)
      }
}

impl Entry {
      pub fn get(self, level: Level) -> (Option<PAddr>, Attr) {
            let attr = Attr::from(self);
            let phys = NonZeroUsize::new((self.0 & level.addr_mask()) as usize);
            (phys, attr)
      }

      pub fn new(phys: Option<PAddr>, attr: Attr, level: Level) -> Self {
            let phys = phys.map_or(0, |phys| phys.get() as u64) & level.addr_mask();
            Entry(phys | attr.bits)
      }

      pub fn reset(&mut self) {
            self.0 = 0;
      }

      pub(crate) fn get_table(&self, id_off: usize, level: Level) -> Option<NonNull<[Entry]>> {
            let (phys, attr) = self.get(level);
            if attr.contains(Attr::PRESENT) && attr.has_table(level) {
                  NonNull::new((phys.map_or(0, |p| p.get()) + id_off) as *mut Entry)
                        .map(|r| NonNull::slice_from_raw_parts(r, NR_ENTRIES))
            } else {
                  None
            }
      }

      pub fn is_leaf(&self, level: Level) -> bool {
            let (phys, attr) = self.get(level);
            phys.is_some() && attr.contains(level.leaf_attr(Attr::empty()))
      }
}

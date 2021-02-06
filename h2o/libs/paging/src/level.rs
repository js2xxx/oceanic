use crate::{LAddr, CANONICAL_PREFIX, NR_ENTRIES, NR_ENTRIES_SHIFT, PAGE_SHIFT, RECURSIVE_IDX};

use core::convert::TryFrom;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(usize)]
pub enum Level {
      Pt = 0,
      Pd = 1,
      Pdp = 2,
      P4 = 3,
      // P5,
}

impl Level {
      pub fn fit(addr: usize) -> Option<Level> {
            for level in Level::P4 {
                  if (addr & (level.page_size() - 1)) == 0 {
                        return Some(level);
                  }
            }
            None
      }

      #[inline]
      pub const fn page_bits(&self) -> usize {
            PAGE_SHIFT + *self as usize * NR_ENTRIES_SHIFT
      }

      #[inline]
      pub const fn page_size(&self) -> usize {
            1usize << self.page_bits()
      }

      #[inline]
      pub const fn recursive_base(&self) -> usize {
            const RECURSIVE_BASE: [usize; 4] = {
                  let r3 = CANONICAL_PREFIX | RECURSIVE_IDX << 39;
                  let r2 = r3 | RECURSIVE_IDX << 30;
                  let r1 = r2 | RECURSIVE_IDX << 21;
                  let r0 = r1 | RECURSIVE_IDX << 12;
                  [r0, r1, r2, r3]
            };
            RECURSIVE_BASE[*self as usize]
      }

      #[inline]
      pub const fn addr_mask(&self) -> u64 {
            const ADDR_MASK: [u64; 4] = [
                  0x0000_FFFF_FFFF_F000,
                  0x0000_FFFF_FFE0_0000,
                  0x0000_FFFF_C000_0000,
                  0x0000_FF80_0000_0000,
            ];
            ADDR_MASK[*self as usize]
      }

      #[inline]
      pub const fn increase(&self) -> Option<Level> {
            match self {
                  Level::Pt => Some(Level::Pd),
                  Level::Pd => Some(Level::Pdp),
                  Level::Pdp => Some(Level::P4),
                  Level::P4 => None,
            }
      }

      #[inline]
      pub const fn decrease(&self) -> Option<Level> {
            match self {
                  Level::Pt => None,
                  Level::Pd => Some(Level::Pt),
                  Level::Pdp => Some(Level::Pd),
                  Level::P4 => Some(Level::Pdp),
            }
      }

      #[inline]
      pub fn leaf_attr(&self, attr: super::Attr) -> super::Attr {
            let pat = attr.contains(super::Attr::PAT);
            let ret = attr
                  | match self {
                        Level::Pt => super::Attr::PRESENT,
                        _ => super::Attr::LARGE_PAGE | super::Attr::PRESENT,
                  };
            if pat && *self != Level::Pt {
                  // `LARGE_PAGE` equals to `PAT`, so we cannot unset the latter.
                  ret | super::Attr::LARGE_PAT
            } else {
                  ret
            }
      }

      #[inline]
      pub fn addr_idx(&self, addr: LAddr, end: bool) -> usize {
            let val = addr.as_ptr() as u64;
            let ret = ((val & self.addr_mask()) >> self.page_bits()) as usize & (NR_ENTRIES - 1);
            if end && ret == 0 {
                  NR_ENTRIES
            } else {
                  ret
            }
      }
}

impl TryFrom<usize> for Level {
      type Error = ();

      fn try_from(value: usize) -> Result<Self, Self::Error> {
            match value {
                  0 => Ok(Level::Pt),
                  1 => Ok(Level::Pd),
                  2 => Ok(Level::Pdp),
                  3 => Ok(Level::P4),
                  _ => Err(()),
            }
      }
}

impl Iterator for Level {
      type Item = Level;

      fn next(&mut self) -> Option<Self::Item> {
            self.decrease()
      }
}

unsafe impl core::iter::Step for Level {
      fn steps_between(start: &Self, end: &Self) -> Option<usize> {
            if *start < *end {
                  None
            } else {
                  Some((*start as usize) - (*end as usize))
            }
      }

      fn forward_checked(start: Self, count: usize) -> Option<Self> {
            let raw = start as usize;
            if raw > count {
                  Some(Self::try_from(raw - count).ok()?)
            } else {
                  None
            }
      }

      fn backward_checked(start: Self, count: usize) -> Option<Self> {
            let raw = start as usize;
            Self::try_from(raw + count).ok()
      }
}

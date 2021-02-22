use crate::{LAddr, CANONICAL_PREFIX, NR_ENTRIES, NR_ENTRIES_SHIFT, PAGE_SHIFT, RECURSIVE_IDX};

use core::convert::TryFrom;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(usize)]
pub enum Level {
      P4 = 3,
      Pdp = 2,
      Pd = 1,
      Pt = 0,
      // P5,
}

impl Level {
      pub fn fit(val: usize) -> Option<Level> {
            log::trace!("paging::Level::fit: val = {:#x}", val);
            if (val & (Level::Pdp.page_size() - 1)) == 0 {
                  return Some(Level::Pdp);
            }
            if (val & (Level::Pd.page_size() - 1)) == 0 {
                  return Some(Level::Pd);
            }
            if (val & (Level::Pt.page_size() - 1)) == 0 {
                  return Some(Level::Pt);
            }
            log::warn!("paging::Level::fit: unknown level");
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
            let ret = attr | super::Attr::PRESENT
                  | match self {
                        Level::Pt => super::Attr::empty(),
                        _ => super::Attr::LARGE_PAGE,
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
            let val = *addr as u64;
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

use crate::{LAddr, PAddr};

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

impl Error {
      pub(crate) fn is_misaligned_invalid(&self) -> bool {
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
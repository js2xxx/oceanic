use paging::PAddr;

use alloc::vec::Vec;
use core::ops::Range;

pub enum PObject {
      Contiguous { range: Range<PAddr> },
      Separate { level: usize, pages: Vec<PAddr> },
}

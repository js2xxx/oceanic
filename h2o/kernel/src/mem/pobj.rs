use paging::PAddr;

use alloc::vec::Vec;
use core::ops::Range;

pub struct PObject {
      range: Range<PAddr>,
}
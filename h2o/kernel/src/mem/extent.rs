use canary::Canary;
use paging::{LAddr, PAddr};

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use bitflags::bitflags;
use core::fmt::Debug;
use core::ops::Range;
use spin::Mutex;

bitflags! {
      pub struct Flags: u32 {
            const USER_ACCESS = 1;
            const READABLE    = 1 << 1;
            const WRTIEABLE   = 1 << 2;
            const EXECUTABLE  = 1 << 3;
      }
}

pub enum Type {
      Region()
}

#[inline]
fn range_include<T: PartialOrd>(value: &Range<T>, universe: &Range<T>) -> bool {
      universe.start <= value.start && value.end <= universe.end
}

#[inline]
fn range_intersect<T: PartialOrd>(a: &Range<T>, b: &Range<T>) -> bool {
      !(a.end < b.start || b.end < a.start)
}

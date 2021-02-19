use canary::Canary;
use paging::{LAddr, PAddr};

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use bitflags::bitflags;
use core::fmt::Debug;
use spin::Mutex;

pub type RangeRef = Arc<Range>;

type AddrRange = core::ops::Range<LAddr>;

bitflags! {
      pub struct RangeFlags: u32 {
            const USER_ACCESS = 1;
            const READABLE    = 1 << 1;
            const WRTIEABLE   = 1 << 2;
            const EXECUTABLE  = 1 << 3;
      }
}

pub enum Mappability {
      Mappable {
            level: Option<usize>,
      },
      Unmappable {
            subranges: BTreeMap<LAddr, RangeRef>,
      },
      Unknown,
}

pub struct Range {
      canary: Canary<Range>,

      range: AddrRange,
      flags: RangeFlags,
      mappability: Mutex<Mappability>,
}

pub enum RangeError {
      AlreadySet,
      AlreadyMapped,
      OutOfRange {
            value: AddrRange,
            universe: AddrRange,
      },
      Other(&'static str)
}

impl Range {
      pub fn new(range: AddrRange, flags: RangeFlags) -> RangeRef {
            Arc::new(Range {
                  canary: Canary::new(),
                  range,
                  flags,
                  mappability: Mutex::new(Mappability::Unknown),
            })
      }

      pub fn create_subrange(
            &self,
            range: AddrRange,
            flags: RangeFlags,
      ) -> Result<RangeRef, RangeError> {
            if !range_include(&range, &self.range) {
                  return Err(RangeError::OutOfRange {
                        value: range,
                        universe: self.range.clone(),
                  });
            }

            let flags = flags & self.flags;
            let key = range.start;

            let mut mappability = self.mappability.lock();
            match &*mappability {
                  Mappability::Unmappable { subranges } => {
                        for (_, item) in subranges
                              .range((core::ops::Bound::Unbounded, core::ops::Bound::Included(key)))
                        {
                              if range_intersect(&item.range, &range) {
                                    return Err(RangeError::AlreadyMapped);
                              }
                        }

                        let ret = Arc::new(Range {
                              canary: Canary::new(),
                              range,
                              flags,
                              mappability: Mutex::new(Mappability::Unknown),
                        });

                        subranges.insert(key, ret.clone());
                        Ok(ret)
                  }
                  Mappability::Unknown => {
                        let ret = Arc::new(Range {
                              canary: Canary::new(),
                              range,
                              flags,
                              mappability: Mutex::new(Mappability::Unknown),
                        });

                        let mut subranges = BTreeMap::new();
                        subranges.insert(key, ret.clone());
                        *mappability = Mappability::Unmappable { subranges };

                        Ok(ret)
                  }
                  Mappability::Mappable { .. } => Err(RangeError::AlreadySet),
            }
      }

      pub fn map_contiguous(&self, base_phys: PAddr) -> Result<(), RangeError> {
            let mut mappability = self.mappability.lock();
            match &*mappability {
                  Mappability::Unknown => {
                        *mappability = Mappability::Mappable { level: None };
                        
                        Ok(())
                  }
                  _ => Err(RangeError::AlreadySet),
            }
      }
}

impl Debug for Range {
      fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "{:?}", self.canary)?;
            if self.canary.check() {
                  write!(f, "{:?}", self.range)?;
            }
            Ok(())
      }
}

#[inline]
fn range_include<T: PartialOrd>(
      value: &core::ops::Range<T>,
      universe: &core::ops::Range<T>,
) -> bool {
      universe.start <= value.start && value.end <= universe.end
}

#[inline]
fn range_intersect<T: PartialOrd>(a: &core::ops::Range<T>, b: &core::ops::Range<T>) -> bool {
      !(a.end < b.start || b.end < a.start)
}

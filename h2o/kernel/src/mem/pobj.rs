use collection_ex::RangeSet;
use paging::PAddr;

use core::ops::Range;

#[derive(Debug)]
pub struct PObject {
      ranges: RangeSet<PAddr>,
      size: usize,
      flags: super::extent::Flags,
}

impl PObject {
      pub fn new(flags: super::extent::Flags) -> PObject {
            PObject {
                  ranges: RangeSet::new(),
                  size: 0,
                  flags,
            }
      }

      pub fn add_range(&mut self, range: Range<PAddr>) -> Result<(), &'static str> {
            self.ranges
                  .insert(range.clone())
                  .map(|_| self.size += *range.end - *range.start)
      }

      pub fn remove_range(&mut self, start: PAddr) -> Option<Range<PAddr>> {
            self.ranges.remove(start).map(|ret| {
                  self.size -= *ret.end - *ret.start;
                  ret
            })
      }

      pub fn size(&self) -> usize {
            self.size
      }

      pub fn flags(&self) -> super::extent::Flags {
            self.flags
      }

      pub fn addr_ranges(&self) -> collection_ex::range_set::RangeIter<'_, PAddr> {
            self.ranges.range_iter()
      }

      pub fn addr_gaps(&self) -> collection_ex::range_set::GapIter<'_, PAddr> {
            self.ranges.gap_iter()
      }
}

use super::space::Space;
use canary::Canary;
use paging::LAddr;

use alloc::collections::BTreeMap;
use alloc::sync::{Arc, Weak};
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

#[derive(Debug)]
pub enum Type {
      Region(BTreeMap<LAddr, Arc<Extent>>),
      Mapping { level: Option<usize> },
}

#[derive(Debug)]
struct Parameters {
      flags: Flags,
      ty: Type,
}

#[derive(Debug)]
pub struct Extent {
      canary: Canary<Extent>,
      range: Range<LAddr>,
      pub(super) space: Weak<Space>,
      parameters: Mutex<Parameters>,
}

impl Extent {
      pub(super) fn new(space: Weak<Space>, range: Range<LAddr>, flags: Flags, ty: Type) -> Self {
            Extent {
                  canary: Canary::new(),
                  range,
                  space,
                  parameters: Mutex::new(Parameters { flags, ty }),
            }
      }

      pub fn create_sub(
            &self,
            range: Range<LAddr>,
            flags: Flags,
            ty: Type,
      ) -> Result<Arc<Self>, ()> {
            let mut param = self.parameters.lock();
            let flags = flags & param.flags;

            // Self must be a region.
            let subextents = match &mut param.ty {
                  Type::Region(subextents) => subextents,
                  _ => return Err(()),
            };

            // Subextents cannot exceed the boundary of its parent.
            if !range_include(&range, &self.range) {
                  return Err(());
            }

            // Subextents cannot overlap each other.
            let key = range.start;
            for (_, item) in subextents.range(..key) {
                  if range_intersect(&range, &item.range) {
                        return Err(());
                  }
            }

            let ret = Arc::new(Extent::new(self.space.clone(), range, flags, ty));
            subextents.insert(key, ret.clone());

            Ok(ret)
      }
}

#[inline]
fn range_include<T: PartialOrd>(value: &Range<T>, universe: &Range<T>) -> bool {
      universe.start <= value.start && value.end <= universe.end
}

#[inline]
fn range_intersect<T: PartialOrd>(a: &Range<T>, b: &Range<T>) -> bool {
      !(a.end < b.start || b.end < a.start)
}

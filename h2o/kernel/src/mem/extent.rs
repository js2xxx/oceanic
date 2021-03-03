use super::pobj::PObject;
use super::space::Space;
use canary::Canary;
use paging::LAddr;

use alloc::collections::BTreeMap;
use alloc::sync::{Arc, Weak};
use bitflags::bitflags;
use core::fmt::Debug;
use core::ops::Range;
use spin::{Mutex, RwLock};

bitflags! {
      pub struct Flags: u32 {
            const USER_ACCESS = 1;
            const READABLE    = 1 << 1;
            const WRTIEABLE   = 1 << 2;
            const EXECUTABLE  = 1 << 3;
      }
}

#[derive(Debug)]
pub enum Error {
      IsRegion(bool),
      RangeInvalid(Range<LAddr>),
      SpaceFull(usize),
      Paging(paging::Error),
      SpaceUnavailable,
}

#[derive(Debug)]
pub enum Type {
      Region(BTreeMap<LAddr, Arc<Extent>>),
      Mapping(PObject, bool),
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
      pub(super) space: RwLock<Weak<Space>>,
      parameters: Mutex<Parameters>,
}

impl Extent {
      pub(super) fn new(space: Weak<Space>, range: Range<LAddr>, flags: Flags, ty: Type) -> Self {
            Extent {
                  canary: Canary::new(),
                  range,
                  space: RwLock::new(space),
                  parameters: Mutex::new(Parameters { flags, ty }),
            }
      }

      pub fn create_subregion(
            &self,
            range: Range<LAddr>,
            flags: Flags,
      ) -> Result<Arc<Self>, Error> {
            let mut param = self.parameters.lock();
            let flags = flags & param.flags;

            let subextents = match &mut param.ty {
                  Type::Region(subextents) => subextents,
                  _ => return Err(Error::IsRegion(false)),
            };

            // Subextents cannot exceed the boundary of its parent.
            if !range_include(&range, &self.range) {
                  return Err(Error::RangeInvalid(range));
            }

            // Subextents cannot overlap each other.
            let key = range.start;
            for (_, item) in subextents.range(..key) {
                  if range_intersect(&range, &item.range) {
                        return Err(Error::RangeInvalid(range));
                  }
            }

            let ret = Arc::new(Extent::new(
                  self.space.read().clone(),
                  range,
                  flags,
                  Type::Region(BTreeMap::new()),
            ));
            subextents.insert(key, ret.clone());

            Ok(ret)
      }

      pub fn create_mapping(&self, obj: PObject, commit: bool) -> Result<Arc<Self>, Error> {
            let mut param = self.parameters.lock();
            let flags = obj.flags() & param.flags;

            let subextents = match &mut param.ty {
                  Type::Region(subextents) => subextents,
                  _ => return Err(Error::IsRegion(false)),
            };

            let size = obj.size();

            // Allocate an available range for the `PObject`.
            let mut last_end = self.range.start;
            let mut range = None;
            for (_, extent) in subextents.iter() {
                  if unsafe { extent.range.start.offset_from(*last_end) } as usize >= size {
                        range = Some(last_end..LAddr::new(unsafe { last_end.add(size) }));
                  }
                  last_end = extent.range.end;
            }
            if unsafe { self.range.end.offset_from(*last_end) } as usize >= size {
                  range = Some(last_end..LAddr::new(unsafe { last_end.add(size) }));
            }

            let range = range.map_or(Err(Error::SpaceFull(size)), Ok)?;
            let key = range.start;

            let ret = Arc::new(Extent::new(
                  self.space.read().clone(),
                  range,
                  flags,
                  Type::Mapping(obj, false),
            ));
            subextents.insert(key, ret.clone());

            if commit {
                  ret.commit_mapping()?;
            }
            Ok(ret)
      }

      pub fn commit_mapping(&self) -> Result<(), Error> {
            let param = self.parameters.lock();
            let pobj = match param.ty {
                  Type::Mapping(ref pobj, false) => pobj,
                  Type::Mapping(_, true) => return Ok(()),
                  _ => return Err(Error::IsRegion(true)),
            };

            let space = self
                  .space
                  .read()
                  .upgrade()
                  .map_or(Err(Error::SpaceUnavailable), Ok)?;
            let mut start = self.range.start;
            for Range {
                  start: phys,
                  end: phys_end,
            } in pobj.addr_ranges()
            {
                  let size = *phys_end - *phys;
                  let virt = start..LAddr::new(unsafe { start.add(size) });

                  space.maps(virt.clone(), phys, param.flags)
                        .map_err(|e| Error::Paging(e))?;

                  start = virt.end;
            }

            Ok(())
      }

      pub fn decommit_mapping(&self) -> Result<(), Error> {
            let param = self.parameters.lock();
            match param.ty {
                  Type::Mapping(_, true) => {
                        let space = self
                              .space
                              .read()
                              .upgrade()
                              .map_or(Err(Error::SpaceUnavailable), Ok)?;
                        let _ = space.unmaps(self.range.clone());
                        Ok(())
                  }
                  Type::Mapping(_, false) => Ok(()),
                  _ => Err(Error::IsRegion(true)),
            }
      }

      pub fn space(&self) -> Weak<Space> {
            self.space.read().clone()
      }
}

impl Drop for Extent {
      fn drop(&mut self) {
            let _ = self.decommit_mapping();
      }
}

#[inline]
fn range_include<T: PartialOrd>(value: &Range<T>, universe: &Range<T>) -> bool {
      universe.start <= value.start && value.end <= universe.end
}

#[inline]
fn range_intersect<T: PartialOrd>(a: &Range<T>, b: &Range<T>) -> bool {
      !(a.end <= b.start || b.end <= a.start)
}

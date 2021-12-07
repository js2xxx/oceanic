use alloc::{boxed::Box, collections::BTreeMap, vec::Vec};
use core::any::Any;

use solvent::Handle;

#[derive(Debug)]
pub struct HandleMap {
    next_id: u32,
    map: BTreeMap<Handle, Box<dyn Any>>,
}

impl HandleMap {
    pub fn new() -> Self {
        HandleMap {
            next_id: 1,
            map: BTreeMap::new(),
        }
    }

    pub fn insert<T: 'static>(&mut self, obj: T) -> Handle {
        let k = box obj;
        let id = {
            let new = self.next_id;
            self.next_id += 1;
            Handle::new(new)
        };
        self.map.insert(id, k);
        id
    }
    pub fn get<T: 'static>(&self, hdl: Handle) -> Option<&T> {
        self.map.get(&hdl).and_then(|k| k.downcast_ref())
    }

    pub fn get_mut<T: 'static>(&mut self, hdl: Handle) -> Option<&mut T> {
        self.map.get_mut(&hdl).and_then(|k| k.downcast_mut())
    }

    pub fn remove<T: 'static>(&mut self, hdl: Handle) -> Option<T> {
        match self.map.entry(hdl) {
            alloc::collections::btree_map::Entry::Occupied(ent)
                if ent.get().downcast_ref::<T>().is_some() =>
            {
                Some(Box::into_inner(ent.remove().downcast().unwrap()))
            }
            _ => None,
        }
    }

    pub fn drop(&mut self, hdl: Handle) {
        if let alloc::collections::btree_map::Entry::Occupied(ent) = self.map.entry(hdl) {
            drop(ent.remove())
        }
    }

    pub fn send(&mut self, handles: &[Handle]) -> Option<Vec<Box<dyn Any>>> {
        for hdl in handles {
            if self.map.get(&hdl).is_none() {
                return None;
            }
        }
        Some(
            handles
                .into_iter()
                .map(|hdl| self.map.remove(&hdl).unwrap())
                .collect(),
        )
    }

    pub fn receive(&mut self, objects: Vec<Box<dyn Any + Send>>) -> Vec<Handle> {
        objects.into_iter().map(|obj| self.insert(obj)).collect()
    }
}

mod syscall {
    use solvent::*;

    #[syscall]
    fn object_drop(hdl: Handle) {
        hdl.check_null()?;
        crate::sched::SCHED.with_current(|cur| {
            let info = cur.tid().info();
            (*info.handles().write()).drop(hdl);
        });
        Ok(())
    }
}

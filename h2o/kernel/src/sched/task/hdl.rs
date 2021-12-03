use alloc::{boxed::Box, collections::BTreeMap};
use core::any::Any;

use solvent::Handle;

#[derive(Debug)]
pub struct HandleMap {
    next_id: u32,
    map: BTreeMap<Handle, Box<dyn Any>>,
}

unsafe impl Send for HandleMap {}
unsafe impl Sync for HandleMap {}

impl HandleMap {
    pub fn new() -> Self {
        HandleMap {
            next_id: 1,
            map: BTreeMap::new(),
        }
    }

    pub fn insert<T: 'static>(&mut self, obj: T) -> Option<Handle> {
        let k = box obj;
        let id = {
            let new = self.next_id;
            self.next_id += 1;
            Handle::new(new)
        };
        self.map.insert(id, k);
        Some(id)
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
}

mod syscall {
    use solvent::*;

    #[syscall]
    fn object_drop(hdl: Handle) {
        hdl.check_null()?;
        crate::sched::SCHED.with_current(|cur| {
            let mut info = cur.tid().info().write();
            info.handles.drop(hdl);
        });
        Ok(())
    }
}

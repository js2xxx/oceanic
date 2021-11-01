use alloc::{boxed::Box, collections::BTreeMap};
use core::{any::Any, num::NonZeroUsize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserHandle(usize);

impl UserHandle {
    pub const NULL: Self = UserHandle(0);

    pub const fn new(raw: NonZeroUsize) -> UserHandle {
        UserHandle(raw.get())
    }

    pub fn raw(&self) -> usize {
        self.0
    }
}

#[derive(Debug)]
pub struct UserHandles {
    next_id: usize,
    map: BTreeMap<usize, Box<dyn Any>>,
}

unsafe impl Send for UserHandles {}
unsafe impl Sync for UserHandles {}

impl UserHandles {
    pub const fn new() -> Self {
        UserHandles {
            next_id: 1,
            map: BTreeMap::new(),
        }
    }

    pub fn insert<T: 'static>(&mut self, obj: T) -> UserHandle {
        let k = box obj;
        let id = self.next_id;
        self.next_id += 1;
        self.map.insert(id, k);
        UserHandle(id)
    }

    pub fn get<T: 'static>(&self, hdl: UserHandle) -> Option<&T> {
        self.map.get(&hdl.0).and_then(|k| k.downcast_ref())
    }

    pub fn get_mut<T: 'static>(&mut self, hdl: UserHandle) -> Option<&mut T> {
        self.map.get_mut(&hdl.0).and_then(|k| k.downcast_mut())
    }

    pub fn remove<T: 'static>(&mut self, hdl: UserHandle) -> Option<T> {
        match self.map.entry(hdl.0) {
            alloc::collections::btree_map::Entry::Occupied(ent)
                if ent.get().downcast_ref::<T>().is_some() =>
            {
                Some(Box::into_inner(ent.remove().downcast().unwrap()))
            }
            _ => None,
        }
    }
}

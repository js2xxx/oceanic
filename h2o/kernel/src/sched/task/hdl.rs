use alloc::{boxed::Box, collections::BTreeMap};
use core::{any::Any, num::NonZeroU32};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct UserHandle(u32);

impl UserHandle {
    pub const NULL: Self = UserHandle(0);

    pub const fn new(raw: NonZeroU32) -> UserHandle {
        UserHandle(raw.get())
    }

    pub fn raw(&self) -> u32 {
        self.0
    }
}

#[derive(Debug)]
pub struct UserHandles {
    next_id: u32,
    map: BTreeMap<UserHandle, Box<dyn Any>>,
}

unsafe impl Send for UserHandles {}
unsafe impl Sync for UserHandles {}

impl UserHandles {
    pub fn new() -> Self {
        UserHandles {
            next_id: 1,
            map: BTreeMap::new(),
        }
    }

    pub fn insert<T: 'static>(&mut self, obj: T) -> Option<UserHandle> {
        let k = box obj;
        let id = {
            let new = self.next_id;
            self.next_id += 1;
            UserHandle(new)
        };
        self.map.insert(id, k);
        Some(id)
    }

    pub fn get<T: 'static>(&self, hdl: UserHandle) -> Option<&T> {
        self.map.get(&hdl).and_then(|k| k.downcast_ref())
    }

    pub fn get_mut<T: 'static>(&mut self, hdl: UserHandle) -> Option<&mut T> {
        self.map.get_mut(&hdl).and_then(|k| k.downcast_mut())
    }

    pub fn remove<T: 'static>(&mut self, hdl: UserHandle) -> Option<T> {
        match self.map.entry(hdl) {
            alloc::collections::btree_map::Entry::Occupied(ent)
                if ent.get().downcast_ref::<T>().is_some() =>
            {
                Some(Box::into_inner(ent.remove().downcast().unwrap()))
            }
            _ => None,
        }
    }
}

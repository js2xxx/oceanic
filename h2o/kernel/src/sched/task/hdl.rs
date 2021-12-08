use alloc::{
    boxed::Box,
    collections::{btree_map, BTreeMap},
    vec::Vec,
};
use core::any::Any;

use solvent::Handle;

use crate::sched::ipc::Channel;

#[derive(Debug)]
pub struct HandleMap {
    next_id: u32,
    map: BTreeMap<Handle, Box<dyn Any>>,
}

unsafe impl Send for HandleMap {}
unsafe impl Sync for HandleMap {}

impl HandleMap {
    #[inline]
    pub fn new() -> Self {
        HandleMap {
            next_id: 1,
            map: BTreeMap::new(),
        }
    }

    #[inline]
    pub fn insert<T: Send + 'static>(&mut self, obj: T) -> Handle {
        unsafe { self.insert_unchecked(obj) }
    }

    /// # Safety
    ///
    /// The caller is responsible for the usage of the inserted object if its
    /// `!Send`.
    pub unsafe fn insert_unchecked<T: 'static>(&mut self, obj: T) -> Handle {
        self.insert_boxed(box obj)
    }

    unsafe fn insert_boxed(&mut self, boxed: Box<dyn Any>) -> Handle {
        let id = {
            let new = self.next_id;
            self.next_id += 1;
            Handle::new(new)
        };
        let _ret = self.map.insert(id, boxed);
        debug_assert!(_ret.is_none());
        id
    }

    #[inline]
    pub fn get<T: Send + 'static>(&self, hdl: Handle) -> Option<&T> {
        unsafe { self.get_unchecked(hdl) }
    }

    /// # Safety
    ///
    /// The caller must ensure the current task is the owner of the
    /// [`HandleMap`] if the returned object is `!Send`.
    pub unsafe fn get_unchecked<T: 'static>(&self, hdl: Handle) -> Option<&T> {
        self.map.get(&hdl).and_then(|k| k.downcast_ref())
    }

    // pub fn get_mut<T: Send + 'static>(&mut self, hdl: Handle) -> Option<&mut T> {
    //     self.map.get_mut(&hdl).and_then(|k| k.downcast_mut())
    // }

    pub fn remove<T: Send + 'static>(&mut self, hdl: Handle) -> Option<T> {
        match self.map.entry(hdl) {
            btree_map::Entry::Occupied(ent) if ent.get().downcast_ref::<T>().is_some() => {
                Some(Box::into_inner(ent.remove().downcast().unwrap()))
            }
            _ => None,
        }
    }

    /// # Safety
    ///
    /// The caller must ensure the current task is the owner of the
    /// [`HandleMap`] if the dropped object is `!Send`.
    pub unsafe fn drop_unchecked(&mut self, hdl: Handle) {
        if let btree_map::Entry::Occupied(ent) = self.map.entry(hdl) {
            drop(ent.remove())
        }
    }

    /// # Safety
    ///
    /// The caller must ensure every object indexed by `handles` in the map is
    /// `Send`, excluding:
    /// * [`crate::mem::space::Virt`].
    pub unsafe fn send_for_channel<'a, 'b>(
        &'a mut self,
        handles: &'b [Handle],
        chan: Handle,
    ) -> Result<(Vec<Box<dyn Any + Send>>, &'a Channel), solvent::Error> {
        debug_assert!(!handles.contains(&chan));

        let chan = self
            .get::<Channel>(chan)
            .ok_or(solvent::Error(solvent::EINVAL))? as *const Channel;

        for hdl in handles {
            match self.map.get(&hdl) {
                None => return Err(solvent::Error(solvent::EINVAL)),
                // TODO: Find a better way to check if the object is `!Send`. If found, remove the
                // `unsafe` marker.
                Some(obj) => {
                    if obj.downcast_ref::<crate::mem::space::Virt>().is_some() {
                        return Err(solvent::Error(solvent::EPERM));
                    }

                    if let Some(other) = obj.downcast_ref::<Channel>() {
                        let chan = unsafe { &*chan };
                        if chan.is_peer(other) {
                            return Err(solvent::Error(solvent::EPERM));
                        }
                    }
                }
            }
        }
        Ok((
            handles
                .into_iter()
                .map(|hdl| self.map.remove(&hdl).unwrap())
                .map(|obj| unsafe {
                    // SAFE: So far, all the objects excluding `Virt` is `Send`. Hence, after
                    // kicking out these shit, we directly reinterpret the pointers just like what
                    // `Box::downcast` does.
                    let (raw, alloc): (*mut dyn Any, _) = Box::into_raw_with_allocator(obj);
                    Box::from_raw_in(raw as *mut (dyn Any + Send), alloc)
                })
                .collect(),
            unsafe { &*chan },
        ))
    }

    pub fn receive(&mut self, objects: Vec<Box<dyn Any + Send>>) -> Vec<Handle> {
        objects
            .into_iter()
            .map(|obj| unsafe { self.insert_boxed(obj) })
            .collect()
    }
}

mod syscall {
    use solvent::*;

    #[syscall]
    fn obj_drop(hdl: Handle) {
        hdl.check_null()?;
        crate::sched::SCHED.with_current(|cur| {
            let info = cur.tid().info();
            unsafe { info.handles().write().drop_unchecked(hdl) };
        });
        Ok(())
    }
}

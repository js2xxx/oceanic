use alloc::{boxed::Box, vec::Vec};
use core::any::Any;

use solvent::Handle;

use crate::sched::SCHED;

#[derive(Debug)]
pub struct Message {
    objects: Vec<Box<dyn Any + Send>>,
    buffer: Box<[u8]>,
}

impl Message {
    pub fn new(objects: Vec<Box<dyn Any + Send>>, data: &[u8]) -> Self {
        let buffer = data.to_vec().into_boxed_slice();
        Message { objects, buffer }
    }

    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    pub fn process(self) -> Option<Vec<Handle>> {
        SCHED.with_current(|cur| {
            let ti = cur.tid().info();
            ti.handles().write().receive(self.objects)
        })
    }
}

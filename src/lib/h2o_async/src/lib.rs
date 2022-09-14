#![no_std]

mod dev;
mod ipc;

extern crate alloc;

use core::task::Waker;

use solvent::prelude::Dispatcher;

pub use self::{dev::Interrupt, ipc::Channel};

fn disp<'a>() -> &'a Dispatcher {
    todo!()
}

fn push_task(_key: usize, _waker: &Waker) {}

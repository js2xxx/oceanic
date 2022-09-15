#![no_std]

mod dev;
mod disp;
mod ipc;

extern crate alloc;

use core::task::Waker;

pub use self::{dev::Interrupt, disp::Dispatcher, ipc::Channel};

fn disp<'a>() -> &'a solvent::prelude::Dispatcher {
    todo!()
}

fn disp2<'a>() -> &'a Dispatcher<'a> {
    todo!()
}

fn push_task(_key: usize, _waker: &Waker) {}

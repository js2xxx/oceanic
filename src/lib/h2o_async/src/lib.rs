#![no_std]

mod dev;
mod ipc;

extern crate alloc;

use core::task::Waker;

use solvent::prelude::{Waiter, ETIME};

pub use self::{dev::Interrupt, ipc::Channel};

fn push_task(waiter: Waiter, waker: &Waker) {
    let _task = move |duration| {
        let res = waiter.end_wait(duration);
        let res = match res {
            Ok(_) => Ok(true),
            Err(ETIME) => Ok(false),
            Err(err) => Err(err),
        };
        waker.wake_by_ref();
        res
    };
}

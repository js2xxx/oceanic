#![no_std]
#![no_main]

use alloc::vec;
use core::time::Duration;

use solvent_std::{sync::Mutex, thread::sleep};

extern crate alloc;

fn main() {
    log::debug!("Hello world!");
    solvent_std::env::args().for_each(|arg| log::debug!("{arg}"));

    let vec = Mutex::new(vec![]);
    let vr = &vec;
    // Sleep sort.
    solvent_std::thread::scope(|s| {
        for i in 0..10 {
            s.spawn(move || {
                sleep(Duration::from_millis((100 - i) * 10));
                vr.lock().push(i)
            });
        }
    });
    log::debug!("{vec:?}");
}

solvent_std::entry!(main);

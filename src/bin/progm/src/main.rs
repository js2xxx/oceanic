#![no_std]
#![no_main]

use alloc::vec;
use core::time::Duration;

use libr::{sync::Mutex, task::sleep};

extern crate alloc;

fn main() {
    log::debug!("Hello world!");
    libr::env::args().for_each(|arg| log::debug!("{arg}"));

    let vec = Mutex::new(vec![]);
    let vr = &vec;
    // Sleep sort.
    libr::task::scope(|s| {
        for i in 0..10 {
            s.spawn(move || {
                sleep(Duration::from_millis((100 - i) * 10));
                vr.lock().push(i)
            });
        }
    });
    log::debug!("{vec:?}");
}

libr::entry!(main);

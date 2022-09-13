#![no_std]
#![no_main]

use alloc::vec::Vec;
use core::cell::RefCell;

use libr::thread_local;

extern crate alloc;

fn main() {
    log::debug!("Hello world!");
    libr::env::args().for_each(|arg| log::debug!("{arg}"));

    let j = libr::task::spawn(|| {
        log::debug!("Hello from an-another thread");
        VEC.with_borrow_mut(|vec| vec.extend_from_slice(&[1, 2, 3, 4, 5]));
        let mut vec = alloc::vec![6, 7, 8, 9, 0];
        log::debug!("{vec:?}");
        VEC.with_borrow(|v| vec.extend_from_slice(v));
        vec
    });
    let vec = j.join();
    log::debug!("Received {vec:?}");
}

libr::entry!(main);

thread_local!(static VEC: RefCell<Vec<u32>> = RefCell::new(Vec::new()));

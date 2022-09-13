#![no_std]
#![no_main]
#![feature(thread_local)]

extern crate alloc;

fn main() {
    log::debug!("Hello world!");
    libr::env::args().for_each(|arg| log::debug!("{arg}"));

    let j = libr::task::spawn(|| {
        log::debug!("Hello from an-another thread");
        let vec = alloc::vec![6, 7, 8, 9, 0];
        log::debug!("{vec:?}");
        vec
    });
    let vec = j.join();
    log::debug!("Received {vec:?}");
}

libr::entry!(main);

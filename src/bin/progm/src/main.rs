#![no_std]
#![no_main]

fn main() {
    log::debug!("Hello world!");
    libr::env::args().for_each(|arg| log::debug!("{arg}"));
}

libr::entry!(main);

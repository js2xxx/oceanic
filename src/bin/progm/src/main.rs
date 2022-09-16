#![no_std]
#![no_main]

extern crate alloc;

fn main() {
    log::debug!("Hello world!");
    solvent_std::env::args().for_each(|arg| log::debug!("{arg}"));

    solvent_async::test::test_disp().expect("Failed to test async dispatcher");
}

solvent_std::entry!(main);

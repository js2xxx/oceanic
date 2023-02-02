#![no_std]
#![no_main]
#![feature(allocator_api)]
#![feature(error_in_core)]

mod instance;
mod ffi;

use alloc::boxed::Box;
use solvent_std::env;
use core::error::Error;

extern crate alloc;

fn main() -> Result<(), Box<dyn Error>> {
    let driver = env::args().nth(1).expect("Failed to get the driver path");
    let task = instance::bootstrap(driver.as_ref())?;
    solvent_async::block_on(Some(1), task);
    Ok(())
}

solvent_std::entry!(main);

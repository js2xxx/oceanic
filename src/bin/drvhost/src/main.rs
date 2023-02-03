#![no_std]
#![no_main]
#![feature(allocator_api)]
#![feature(error_in_core)]

mod ffi;
mod instance;

use alloc::boxed::Box;
use core::error::Error;

use solvent_std::env;

extern crate alloc;

fn main() -> Result<(), Box<dyn Error>> {
    let driver = env::args().nth(1).expect("Failed to get the driver path");
    let task = instance::bootstrap(driver.as_ref())?;
    solvent_async::block_on(Some(1), task);
    Ok(())
}

solvent_std::entry!(main);

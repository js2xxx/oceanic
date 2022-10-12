#![no_std]

mod entry;
mod fs;

extern crate alloc;

pub use self::fs::LocalFs;

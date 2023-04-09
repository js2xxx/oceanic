#![no_std]
#![feature(alloc_error_handler)]
#![feature(alloc_layout_extra)]
#![feature(allocator_api)]
#![feature(error_in_core)]
#![feature(int_roundings)]
#![feature(never_type)]
#![feature(thread_local)]

extern crate alloc;

pub mod env;
pub mod rt;
pub use solvent_core::*;
mod alloc2;

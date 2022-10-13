#![no_std]
#![feature(alloc_error_handler)]
#![feature(alloc_layout_extra)]
#![feature(allocator_api)]
#![feature(error_in_core)]
#![feature(int_roundings)]
#![feature(never_type)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(thread_local)]
#![feature(unzip_option)]

extern crate alloc;

pub mod env;
pub mod rt;
pub use solvent_core::*;
mod alloc2;

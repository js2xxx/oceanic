#![no_std]
#![allow(unused_unsafe)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]

pub mod error;
pub mod mem;
pub mod obj;
pub mod time;
pub mod ipc;
pub mod dev;

extern crate alloc;

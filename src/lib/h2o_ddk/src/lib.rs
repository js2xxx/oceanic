#![doc = include_str!("../README.md")]
#![no_std]
#![feature(allocator_api)]
#![feature(nonnull_slice_from_raw_parts)]

#[cfg(feature = "ddk")]
mod alloc2;
pub mod ffi;

#[cfg(feature = "ddk")]
extern crate alloc;

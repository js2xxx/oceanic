#![doc = include_str!("../README.md")]
#![no_std]
#![feature(allocator_api)]
#![feature(nonnull_slice_from_raw_parts)]

#[cfg(feature = "ddk")]
mod alloc2;
#[cfg_attr(feature = "ddk", doc(hidden))]
pub mod ffi;
#[cfg(feature = "ddk")]
pub mod fs;
#[cfg(feature = "ddk")]
pub mod task;

#[cfg(feature = "ddk")]
extern crate alloc;

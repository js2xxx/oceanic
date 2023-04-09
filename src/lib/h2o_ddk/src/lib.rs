#![doc = include_str!("../README.md")]
#![no_std]
#![feature(allocator_api)]

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

#![no_std]
#![warn(clippy::missing_panics_doc)]
#![feature(allocator_api)]
#![feature(lang_items)]
#![feature(linkage)]

pub mod call;
mod error;
pub mod ipc;
pub mod mem;
pub mod res;
pub mod task;

pub use sv_gen::*;

#[cfg(feature = "call")]
pub use self::call::*;
pub use self::{
    call::{hdl::Handle, reg::*},
    error::*,
};

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/target/rxx.rs"));

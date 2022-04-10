#![no_std]
#![warn(clippy::missing_panics_doc)]
#![feature(allocator_api)]
#![feature(lang_items)]
#![feature(linkage)]

pub mod call;
mod error;
pub mod feat;
pub mod ipc;
pub mod mem;
pub mod res;
#[cfg(feature = "stub")]
pub mod stub;
pub mod task;

pub use sv_gen::*;

#[cfg(all(not(feature = "stub"), feature = "call"))]
pub use self::call::*;
#[cfg(feature = "stub")]
pub use self::stub::*;
pub use self::{
    call::{hdl::Handle, reg::*},
    error::*,
    feat::*,
};

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/target/rxx.rs"));

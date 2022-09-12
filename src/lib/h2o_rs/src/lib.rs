#![no_std]
#![allow(unused_unsafe)]
#![feature(atomic_mut_ptr)]
#![feature(int_roundings)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]

pub mod c_ty;
pub mod dev;
pub mod error;
pub mod ipc;
pub mod mem;
pub mod obj;
pub mod sync;
pub mod task;
pub mod time;

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod prelude {
    pub use crate::{dev::*, error::*, ipc::*, mem::*, obj::*, task::*, time::*};
}

#[cfg(all(feature = "call", target = "x86_64-pc-oceanic"))]
compile_error!("The application should only use VDSO");

#[cfg(not(any(feature = "call", feature = "stub")))]
compile_error!("The application should choose only one feature");

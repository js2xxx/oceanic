#![no_std]
#![feature(array_try_from_fn)]
#![feature(error_in_core)]
#![feature(box_into_inner)]
#![feature(extend_one)]
#![feature(iterator_try_collect)]

extern crate alloc;

mod error;
pub mod packet;

pub use solvent_rpc_macros::SerdePacket;

pub use self::error::Error;

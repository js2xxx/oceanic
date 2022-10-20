#![no_std]
#![feature(iterator_try_collect)]

mod sa;
mod statics;

extern crate alloc;

pub use self::{sa::*, statics::*};

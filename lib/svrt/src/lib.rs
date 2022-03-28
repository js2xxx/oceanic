#![no_std]
#![feature(iterator_try_collect)]

mod c_ty;
mod sa;
mod statics;

extern crate alloc;

pub use self::{c_ty::*, sa::*, statics::*};

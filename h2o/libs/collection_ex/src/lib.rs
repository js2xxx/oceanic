#![no_std]
#![feature(bound_cloned)]

pub mod range_set;

pub use range_set::RangeSet;

extern crate alloc;
#![no_std]
#![feature(bound_cloned)]
#![feature(const_btree_new)]
#![feature(const_fn_trait_bound)]

pub mod range_set;
pub mod range_map;

pub use range_set::RangeSet;
pub use range_map::RangeMap;

extern crate alloc;
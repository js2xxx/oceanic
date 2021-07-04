#![no_std]
#![feature(bound_cloned)]
#![feature(const_btree_new)]
#![feature(const_fn_trait_bound)]

pub mod range_map;
pub mod range_set;

pub use range_map::RangeMap;
pub use range_set::RangeSet;

extern crate alloc;

#![no_std]
#![feature(build_hasher_simple_hash_one)]
#![feature(const_btree_new)]
#![feature(const_fn_trait_bound)]

mod chash_map;
mod fnv_hasher;
mod id_alloc;
mod range_map;
mod range_set;

pub use chash_map::CHashMap;
pub use fnv_hasher::FnvHasher;
pub use id_alloc::IdAllocator;
pub use range_map::RangeMap;
pub use range_set::RangeSet;

extern crate alloc;

#![no_std]
#![feature(build_hasher_simple_hash_one)]
#![feature(const_btree_new)]
#![feature(const_fn_trait_bound)]

pub mod chash_map;
pub mod fnv_hasher;
pub mod id_alloc;
pub mod range_map;
pub mod range_set;

pub use chash_map::CHashMap;
pub use fnv_hasher::FnvHasher;
pub use id_alloc::IdAllocator;
pub use range_map::RangeMap;
pub use range_set::RangeSet;

extern crate alloc;

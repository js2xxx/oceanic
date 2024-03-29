#![no_std]
#![feature(build_hasher_simple_hash_one)]
#![feature(map_try_insert)]

mod chash_map;
mod fnv_hasher;
mod id_alloc;
mod range_map;

pub use self::chash_map::CHashMap;
pub type CHashMapReadGuard<'a, K, V, S> = chash_map::ReadGuard<'a, K, V, S>;
pub type CHashMapWriteGuard<'a, K, V, S> = chash_map::WriteGuard<'a, K, V, S>;
pub use self::{fnv_hasher::FnvHasher, id_alloc::IdAllocator};
pub type RangeMap<K, V> = range_map::RangeMap<K, V>;

extern crate alloc;

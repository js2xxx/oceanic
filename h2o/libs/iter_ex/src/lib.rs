#![no_std]

pub mod comb;
pub mod ptr_iter;

pub use comb::{Combine, CombineIter};
pub use ptr_iter::PointerIterator;

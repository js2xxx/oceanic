#![no_std]

pub mod ptr_iter;
pub mod comb;

pub use ptr_iter::PointerIterator;
pub use comb::{Combine, CombineIter};
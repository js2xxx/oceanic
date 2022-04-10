#![no_std]

mod comb;
mod ptr_iter;

pub use self::{
    comb::{Combine, CombineIter},
    ptr_iter::PtrIter,
};

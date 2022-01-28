#![no_std]
#![feature(allocator_api)]
#![feature(bool_to_option)]
#![feature(negative_impls)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(result_into_ok_or_err)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]

pub mod call;
mod error;
pub mod ipc;
pub mod mem;
pub mod res;
pub mod task;

pub use solvent_gen::*;

pub use self::{
    call::{hdl::Handle, reg::*},
    error::*,
};

#[derive(Debug, Copy, Clone)]
pub struct Arguments {
    pub fn_num: usize,
    pub args: [usize; 5],
}

pub type SyscallWrapper = unsafe extern "C" fn(usize, usize, usize, usize, usize) -> usize;

#[cfg(feature = "call")]
pub fn test() {
    #[cfg(debug_assertions)]
    {
        let stack = task::test::test();
        ipc::test::test(stack);
        mem::test();
    }
}

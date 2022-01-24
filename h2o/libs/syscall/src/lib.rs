#![no_std]
#![feature(allocator_api)]
#![feature(bool_to_option)]
#![feature(lang_items)]
#![feature(negative_impls)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(result_into_ok_or_err)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]

mod call;
mod error;
pub mod ipc;
pub mod mem;
pub mod task;
pub mod time;
cfg_if::cfg_if! {
    if #[cfg(feature = "call")] {
        pub mod rxx;
        pub mod log;
    }
}

pub use call::{hdl::Handle, reg::*};
pub use error::*;
pub use solvent_gen::*;

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
        task::test::test();
        ipc::test();
        mem::test();
    }
}

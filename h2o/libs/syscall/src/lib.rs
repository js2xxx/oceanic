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

pub use sv_gen::*;

#[cfg(feature = "call")]
pub use self::call::*;
pub use self::{
    call::{hdl::Handle, reg::*},
    error::*,
};

#[cfg(feature = "call")]
pub fn test() {
    // #[cfg(debug_assertions)]
    {
        let stack = task::test::test();
        ipc::test::test(stack);
        mem::test();
    }
}

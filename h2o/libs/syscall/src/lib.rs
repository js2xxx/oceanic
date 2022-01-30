#![no_std]
#![feature(allocator_api)]

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

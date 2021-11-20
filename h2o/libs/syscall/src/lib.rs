#![no_std]
#![feature(asm)]
#![feature(bool_to_option)]
#![feature(lang_items)]
#![feature(result_into_ok_or_err)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]

pub mod call;
pub mod error;
pub mod time;
cfg_if::cfg_if! {
    if #[cfg(feature = "call")] {
        pub mod rxx;
        pub mod log;
        pub mod task;
        pub mod mem;
    }
}

pub use call::reg::*;
pub use error::*;
pub use solvent_gen::*;

#[derive(Debug, Copy, Clone)]
pub struct Arguments {
    pub fn_num: usize,
    pub args: [usize; 5],
}

pub type SyscallWrapper = unsafe extern "C" fn(usize, usize, usize, usize, usize) -> usize;

#[allow(dead_code)]
#[cfg(feature = "call")]
pub fn test_task() {
    extern "C" fn func(arg: *mut u8) {
        ::log::debug!("New task here: {:?}", arg);
        // for _ in 0..10000000 {}
        crate::task::exit(Ok(12345));
    }
    ::log::debug!("Creating a task");
    let task = crate::call::task_fn(
        core::ptr::null_mut(),
        crate::task::DEFAULT_STACK_SIZE,
        func as *mut u8,
        test_task as *mut u8,
    )
    .expect("Failed to create task");
    // ::log::debug!("Killing a task");
    // crate::call::task_ctl(task, 1).expect("Failed to kill a task");
    ::log::debug!("Waiting a task");
    let ret = crate::call::task_join(task).expect("Failed to join a task");
    ::log::debug!("Return value = {}", ret);
}

#![no_std]
#![allow(unused_unsafe)]
#![feature(alloc_error_handler)]
#![feature(asm_sym)]
#![feature(naked_functions)]

pub mod elf;
pub mod rxx;
mod dso;

use core::alloc::GlobalAlloc;

pub use self::rxx::{dynamic, load_address, init_channel, vdso_map};

fn dl_main() -> rxx::DlReturn {
    dso::init();
    unsafe {
        *(0x12345 as *mut u8) = 0;
    }
    todo!()
}

#[global_allocator]
static NULL_ALLOC: NullAlloc = NullAlloc;

pub struct NullAlloc;

unsafe impl GlobalAlloc for NullAlloc {
    unsafe fn alloc(&self, _: core::alloc::Layout) -> *mut u8 {
        core::ptr::null_mut()
    }

    unsafe fn dealloc(&self, _: *mut u8, _: core::alloc::Layout) {}
}

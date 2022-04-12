#![no_std]
#![allow(unused_unsafe)]
#![feature(alloc_error_handler)]
#![feature(c_variadic)]
#![feature(core_ffi_c)]
#![feature(int_roundings)]
#![feature(linkage)]

pub mod env;
pub mod ffi;

extern crate alloc;

use alloc::alloc::handle_alloc_error;
use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
};

#[panic_handler]
#[linkage = "weak"]
#[no_mangle]
extern "C" fn rust_begin_unwind(info: &core::panic::PanicInfo) -> ! {
    env::__libc_panic(info)
}

/// The function indicating memory runs out.
#[alloc_error_handler]
fn rust_oom(layout: core::alloc::Layout) -> ! {
    log::error!("Allocation error for {:?}", layout);

    loop {
        unsafe { core::arch::asm!("pause; ud2") }
    }
}

#[global_allocator]
static TMP: TempAlloc = TempAlloc {
    buffer: UnsafeCell::new(Buffer([0; BUFFER_SIZE])),
    buffer_index: UnsafeCell::new(0),
};

const BUFFER_SIZE: usize = 512;
#[repr(align(4096))]
struct Buffer([u8; BUFFER_SIZE]);
pub struct TempAlloc {
    buffer: UnsafeCell<Buffer>,
    buffer_index: UnsafeCell<usize>,
}

unsafe impl Send for TempAlloc {}
unsafe impl Sync for TempAlloc {}

unsafe impl GlobalAlloc for TempAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let index = self.buffer_index.get();
        let i = (*index).next_multiple_of(layout.align());
        if i + layout.size() >= BUFFER_SIZE {
            handle_alloc_error(layout)
        } else {
            let ptr = self.buffer.get().cast::<u8>().add(i);
            *index = i + layout.size();
            ptr
        }
    }

    unsafe fn dealloc(&self, _: *mut u8, _: Layout) {}
}

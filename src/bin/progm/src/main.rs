#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

use solvent::prelude::Handle;

#[no_mangle]
unsafe extern "C" fn _start(init_chan: Handle) {
    __libc_start_main(init_chan, main)
}

#[no_mangle]
unsafe extern "C" fn main(_: u32, _: *mut *mut i8, _: *mut *mut i8) -> i32 {
    let ptr = aligned_alloc(core::mem::align_of::<u8>(), core::mem::size_of::<u128>());
    assert!(!ptr.is_null());
    0
}

type Main = unsafe extern "C" fn(argc: u32, argv: *mut *mut i8, environ: *mut *mut i8) -> i32;

#[link(name = "co2")]
extern "C" {
    fn __libc_start_main(init_chan: Handle, main: Main) -> !;

    fn aligned_alloc(align: usize, size: usize) -> *mut u8;
}

#[panic_handler]
fn rust_begin_unwind(info: &core::panic::PanicInfo) -> ! {
    log::error!("{}", info);

    loop {
        unsafe { core::arch::asm!("pause; ud2") }
    }
}

/// The function indicating memory runs out.
#[alloc_error_handler]
fn rust_oom(layout: core::alloc::Layout) -> ! {
    log::error!("Allocation error for {:?}", layout);

    loop {
        unsafe { core::arch::asm!("pause; ud2") }
    }
}

struct NullAlloc;

#[global_allocator]
static NULL_ALLOC: NullAlloc = NullAlloc;

unsafe impl core::alloc::GlobalAlloc for NullAlloc {
    unsafe fn alloc(&self, _: core::alloc::Layout) -> *mut u8 {
        core::ptr::null_mut()
    }

    unsafe fn dealloc(&self, _: *mut u8, _: core::alloc::Layout) {}
}

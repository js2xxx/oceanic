#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

#[no_mangle]
extern "C" fn _start() {
    todo!()
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

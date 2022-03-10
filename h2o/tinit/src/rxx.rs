#[allow(non_snake_case)]
#[no_mangle]
/// Required to handle panics.
pub extern "C" fn _Unwind_Resume() -> ! {
    panic!("_Unwind_Resume")
}

#[panic_handler]
fn rust_begin_unwind(info: &core::panic::PanicInfo) -> ! {
    log::error!("{}", info);

    loop {
        unsafe { core::arch::asm!("pause") }
    }
}

/// The function indicating memory runs out.
#[alloc_error_handler]
fn rust_oom(layout: core::alloc::Layout) -> ! {
    log::error!("!!!! ALLOCATION ERROR !!!!");
    log::error!("Request: {:?}", layout);

    loop {
        unsafe { core::arch::asm!("pause") }
    }
}

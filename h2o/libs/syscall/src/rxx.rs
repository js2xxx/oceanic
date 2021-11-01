#[panic_handler]
pub fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    #[cfg(debug_assertions)]
    log::debug!("{}", info);
    loop {
        unsafe { asm!("pause") }
    }
}

#[lang = "eh_personality"]
pub extern "C" fn rust_eh_personality() {}

#[allow(non_snake_case)]
#[no_mangle]
/// Required to handle panics.
pub extern "C" fn _Unwind_Resume() -> ! {
    panic!()
}

/// The function indicating memory runs out.
#[lang = "oom"]
pub fn out_of_memory(_layout: core::alloc::Layout) -> ! {
    panic!()
}

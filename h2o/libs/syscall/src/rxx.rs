fn temp_panic(addr: usize) -> ! {
    unsafe { *(addr as *mut u32) = 0 };
    loop {
        unsafe { core::arch::asm!("hlt") }
    }
}

// #[cfg(not(test))]
#[panic_handler]
#[linkage = "weak"]
#[no_mangle]
pub extern "C" fn rust_begin_unwind(_: &core::panic::PanicInfo) -> ! {
    // TODO: Send the panic info to the standard output.
    temp_panic(0x123456789ab0)
}

#[lang = "eh_personality"]
#[no_mangle]
#[linkage = "weak"]
pub extern "C" fn rust_eh_personality() {}

#[lang = "oom"]
#[no_mangle]
#[linkage = "weak"]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn rust_oom(_: core::alloc::Layout) -> ! {
    temp_panic(0xba9876543210)
}

#[no_mangle]
#[linkage = "weak"]
#[allow(non_snake_case)]
pub extern "C" fn _Unwind_Resume() -> ! {
    temp_panic(0x1b2a39485760)
}

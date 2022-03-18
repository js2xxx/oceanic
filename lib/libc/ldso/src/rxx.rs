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

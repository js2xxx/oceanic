#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    log::error!("CPU #{} {}", unsafe { crate::cpu::id() }, info);
    unsafe { archop::halt_loop(Some(false)) }
}

/// The function indicating memory runs out.
#[alloc_error_handler]
fn out_of_memory(layout: core::alloc::Layout) -> ! {
    log::error!("!!!! ALLOCATION ERROR !!!!");
    log::error!("Request: {:?}", layout);

    unsafe { archop::halt_loop(None) };
}

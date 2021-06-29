#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
      log::error!("Kernel {}", info);
      unsafe { archop::halt_loop(Some(true)) }
}

#[lang = "eh_personality"]
pub extern "C" fn rust_eh_personality() {}

#[allow(non_snake_case)]
#[no_mangle]
/// Required to handle panics.
pub extern "C" fn _Unwind_Resume() -> ! {
      unsafe { archop::halt_loop(Some(true)) }
}

/// The function indicating memory runs out.
#[lang = "oom"]
fn out_of_memory(layout: core::alloc::Layout) -> ! {
      log::error!("!!!! ALLOCATION ERROR !!!!");
      log::error!("Request: {:?}", layout);

      unsafe { archop::halt_loop(None) };
}

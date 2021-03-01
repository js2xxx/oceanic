#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
      if let Some(location) = info.location() {
            log::error!(
                  "Panic in {} at ({}, {}):",
                  location.file(),
                  location.line(),
                  location.column()
            );
            if let Some(message) = info.message() {
                  log::error!("{}", message);
            }
      }

      loop {
            core::hint::spin_loop();
      }
}

#[alloc_error_handler]
fn out_of_memory(layout: ::core::alloc::Layout) -> ! {
      panic!(
            "Ran out of free memory while trying to allocate {:#?}",
            layout
      );
}

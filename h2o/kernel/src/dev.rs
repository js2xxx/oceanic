pub mod acpi;
mod res;

cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        mod x86_64;
        pub use self::x86_64::*;
    }
}

use alloc::sync::Arc;

use spin::Lazy;

pub use self::res::Resource;
pub use crate::{cpu::intr::gsi_resource, mem::mem_resource};

static PIO_RESOURCE: Lazy<Arc<Resource<u16>>> = Lazy::new(|| {
    let ret = Resource::new(archop::rand::get(), 0..u16::MAX, None);
    core::mem::forget(
        ret.allocate(crate::log::COM_LOG..(crate::log::COM_LOG + 1))
            .expect("Failed to reserve debug port"),
    );
    ret
});

#[inline]
pub fn pio_resource() -> &'static Arc<Resource<u16>> {
    &PIO_RESOURCE
}

/// # Safety
///
/// This function must be called only once from the bootstrap CPU.
#[inline]
pub unsafe fn init() {
    Lazy::force(&PIO_RESOURCE);
    unsafe { x86_64::init_intr_chip() };
}

pub mod acpi;
mod res;

cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        mod x86_64;
        pub use self::x86_64::*;
    }
}

use alloc::sync::Arc;

use archop::Azy;

pub use self::res::Resource;
pub use crate::{cpu::intr::gsi_resource, mem::mem_resource};

static PIO_RESOURCE: Azy<Arc<Resource<u16>>> = Azy::new(|| {
    let ret = Resource::new_root(archop::rand::get(), 0..u16::MAX);
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
    Azy::force(&PIO_RESOURCE);
    unsafe { x86_64::init_intr_chip() };
}

mod syscall {
    use bitvec::bitvec;
    use sv_call::*;

    use super::*;
    use crate::{cpu::arch::KERNEL_GS, sched::SCHED};

    #[syscall]
    fn pio_acq(res: Handle, base: u16, size: u16) -> Result {
        SCHED.with_current(|cur| {
            let res = cur.space().handles().get::<Arc<Resource<u16>>>(res)?;
            if !{ res.feature().lock() }.contains(Feature::READ | Feature::WRITE) {
                return Err(Error::EPERM);
            }
            if res.magic_eq(pio_resource())
                && res.range().start <= base
                && base + size <= res.range().end
            {
                let io_bitmap = cur.io_bitmap_mut().get_or_insert_with(|| bitvec![1; 65536]);
                for item in io_bitmap.iter_mut().skip(base as usize).take(size as usize) {
                    item.set(false);
                }
                unsafe { KERNEL_GS.update_tss_io_bitmap(cur.io_bitmap_mut().as_deref()) };
                Ok(())
            } else {
                Err(Error::EPERM)
            }
        })
    }

    #[syscall]
    fn pio_rel(res: Handle, base: u16, size: u16) -> Result {
        SCHED.with_current(|cur| {
            let res = cur.space().handles().get::<Arc<Resource<u16>>>(res)?;
            if res.magic_eq(pio_resource())
                && res.range().start <= base
                && base + size <= res.range().end
            {
                if let Some(io_bitmap) = cur.io_bitmap_mut() {
                    for item in io_bitmap.iter_mut().skip(base as usize).take(size as usize) {
                        item.set(true);
                    }
                };
                unsafe { KERNEL_GS.update_tss_io_bitmap(cur.io_bitmap_mut().as_deref()) };
                Ok(())
            } else {
                Err(Error::EPERM)
            }
        })
    }
}

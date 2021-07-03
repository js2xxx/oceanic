pub mod intr;
pub mod seg;

use crate::mem::space::Space;

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::pin::Pin;
use spin::Mutex;

#[repr(C)]
pub struct KernelGs {
      save_regs: *mut u8,
      tss_rsp0: *mut u8,
}

fn init_kernel_gs(tss_rsp0: *mut u8) -> Box<KernelGs> {
      let ptr = Box::into_raw(box KernelGs {
            save_regs: intr::ctx::test::save_regs as *mut u8,
            tss_rsp0,
      });
      unsafe {
            // TODO: removing [`test`] in the future.
            intr::ctx::test::init_stack_top(alloc::alloc::alloc(paging::PAGE_LAYOUT));

            use archop::msr;
            msr::write(msr::KERNEL_GS_BASE, ptr as u64);
            Box::from_raw(ptr)
      }
}

/// Initialize x86_64 architecture.
///
/// # Safety
///
/// The caller must ensure that this function should only be called once from bootstrap
/// CPU.
pub unsafe fn init(
      space: &Arc<Space>,
) -> (
      Mutex<seg::ndt::DescTable<'_>>,
      Pin<&mut seg::ndt::TssStruct>,
      Box<KernelGs>,
) {
      let (gdt, tss) = seg::init(space);

      let kernel_gs = init_kernel_gs(*tss.rsp0());

      (gdt, tss, kernel_gs)
}

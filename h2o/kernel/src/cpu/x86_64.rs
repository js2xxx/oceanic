pub mod intr;
pub mod seg;

use crate::mem::space::Space;

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::pin::Pin;
use spin::Mutex;

#[repr(C)]
struct KernelGs {
      save_regs: *mut u8,
      tss_rsp0: *mut u8,
}

fn init_kernel_gs(tss_rsp0: *mut u8) -> Mutex<Box<KernelGs>> {
      let gs_data = {
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
      };

      Mutex::new(gs_data)
}

/// The arch-specific part of a core of a CPU. (x86_64)
pub struct Core<'a> {
      gdt: Mutex<seg::ndt::DescTable<'a>>,
      ldt: (Mutex<seg::ndt::DescTable<'a>>, u16),
      tss: Mutex<Pin<&'a mut seg::ndt::TssStruct>>,
      idt: Mutex<seg::idt::IntDescTable<'a>>,
      kernel_gs: Mutex<Box<KernelGs>>,
}

impl<'a> Core<'a> {
      /// Construct a new arch-specific `Core` object.
      ///
      /// NOTE: This function should only be called once from BSP.
      pub fn new(space: &'a Arc<Space>) -> Self {
            let gdt = seg::ndt::init_gdt(space);
            unsafe { seg::reload_pls() };
            let (gdt, ldt, ldtr) = seg::ndt::init_ldt(space, gdt);
            let (gdt, tss) = seg::ndt::init_tss(space, gdt);
            let idt = seg::idt::init_idt(space);

            let tss_rsp0 = *tss.lock().rsp0();
            let kernel_gs = init_kernel_gs(tss_rsp0);
            Core {
                  gdt,
                  ldt: (ldt, ldtr),
                  tss,
                  idt,
                  kernel_gs,
            }
      }

      #[inline]
      pub fn gdt(&self) -> &Mutex<seg::ndt::DescTable<'a>> {
            &self.gdt
      }

      #[inline]
      pub fn ldt(&self) -> &Mutex<seg::ndt::DescTable<'a>> {
            &self.ldt.0
      }

      #[inline]
      pub fn ldtr(&self) -> u16 {
            self.ldt.1
      }

      #[inline]
      pub fn tss(&self) -> &Mutex<Pin<&'a mut seg::ndt::TssStruct>> {
            &self.tss
      }

      #[inline]
      pub fn idt(&self) -> &Mutex<seg::idt::IntDescTable<'a>> {
            &self.idt
      }
}

// pub fn init() {

// }

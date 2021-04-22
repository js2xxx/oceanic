pub mod intr;
pub mod seg;

use crate::mem::space::Space;

use alloc::sync::Arc;
use spin::Mutex;

/// The arch-specific part of a core of a CPU. (x86_64)
pub struct Core<'a> {
      gdt: Mutex<seg::ndt::DescTable<'a>>,
      ldt: (Mutex<seg::ndt::DescTable<'a>>, u16),
      idt: Mutex<seg::idt::IntDescTable<'a>>,
}

impl<'a> Core<'a> {
      /// Construct a new arch-specific `Core` object.
      ///
      /// NOTE: This function should only be called once from BSP.
      pub fn new(space: &'a Arc<Space>) -> Self {
            let gdt = seg::ndt::init_gdt(space);
            unsafe { seg::reload_pls() };
            let (gdt, ldt, ldtr) = seg::ndt::init_ldt(space, gdt);
            let idt = seg::idt::init_idt(space);
            Core {
                  gdt,
                  ldt: (ldt, ldtr),
                  idt,
            }
      }

      #[inline]
      pub fn gdt(&self) -> &Mutex<seg::ndt::DescTable<'a>> {
            &self.gdt
      }

      #[inline]
      pub fn ldt(&self) -> (&Mutex<seg::ndt::DescTable<'a>>, u16) {
            (&self.ldt.0, self.ldt.1)
      }

      #[inline]
      pub fn idt(&self) -> &Mutex<seg::idt::IntDescTable<'a>> {
            &self.idt
      }
}

// pub fn init() {

// }

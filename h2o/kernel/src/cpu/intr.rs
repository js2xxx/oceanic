pub use super::arch::intr as arch;

use alloc::sync::Arc;
use spin::Mutex;

pub trait IntrChip {
      /// # Safety
      ///
      /// WARNING: This function modifies the architecture's basic registers. Be sure to make
      /// preparations.
      unsafe fn mask(&mut self, intr: Arc<Interrupt>);

      /// # Safety
      ///
      /// WARNING: This function modifies the architecture's basic registers. Be sure to make
      /// preparations.
      unsafe fn unmask(&mut self, intr: Arc<Interrupt>);

      /// # Safety
      ///
      /// WARNING: This function modifies the architecture's basic registers. Be sure to make
      /// preparations.
      unsafe fn ack(&mut self, intr: Arc<Interrupt>);

      /// # Safety
      ///
      /// WARNING: This function modifies the architecture's basic registers. Be sure to make
      /// preparations.
      unsafe fn eoi(&mut self, intr: Arc<Interrupt>);
}

pub struct Interrupt {
      hw_irq: u32,
      arch_reg: Mutex<arch::ArchReg>,
}

impl Interrupt {
      pub fn hw_irq(&self) -> u32 {
            self.hw_irq
      }
}

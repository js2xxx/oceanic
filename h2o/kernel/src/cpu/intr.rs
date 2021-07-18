pub mod alloc;

pub use super::arch::intr as arch;

use ::alloc::sync::Arc;
use ::alloc::vec::Vec;
use spin::Mutex;

bitflags::bitflags! {
      pub struct IrqReturn: u8 {
            const SUCCESS = 0b0001;
            const WAKE_TASK = 0b0010;
      }
}

pub type Handler = fn(Arc<Interrupt>) -> IrqReturn;

pub trait IntrChip {
      //! TODO: Add declaration of `setup` and `remove` for interrupts.

      /// Mask a interrupt so as to forbid it from triggering.
      ///
      /// # Safety
      ///
      /// WARNING: This function modifies the architecture's basic registers. Be sure to make
      /// preparations.
      unsafe fn mask(&mut self, intr: Arc<Interrupt>);

      /// Unmask a interrupt so that it can trigger.
      ///
      /// # Safety
      ///
      /// WARNING: This function modifies the architecture's basic registers. Be sure to make
      /// preparations.
      unsafe fn unmask(&mut self, intr: Arc<Interrupt>);

      /// Acknowledge a interrupt in the beginning of its handler.
      ///
      /// # Safety
      ///
      /// WARNING: This function modifies the architecture's basic registers. Be sure to make
      /// preparations.
      unsafe fn ack(&mut self, intr: Arc<Interrupt>);

      /// Mark the end of the interrupt's handler.
      ///
      /// # Safety
      ///
      /// WARNING: This function modifies the architecture's basic registers. Be sure to make
      /// preparations.
      unsafe fn eoi(&mut self, intr: Arc<Interrupt>);
}

pub struct Interrupt {
      gsi: u32,
      hw_irq: u8,
      arch_reg: Mutex<arch::ArchReg>,
      handler: Vec<Handler>,
      affinity: super::CpuMask,
}

impl Interrupt {
      pub fn gsi(&self) -> u32 {
            self.gsi
      }

      pub fn hw_irq(&self) -> u8 {
            self.hw_irq
      }

      pub fn arch_reg(&self) -> &Mutex<arch::ArchReg> {
            &self.arch_reg
      }

      pub fn handle(self: &Arc<Interrupt>) -> IrqReturn {
            let ret = IrqReturn::empty();
            for hdl in self.handler.iter() {
                  let r = (hdl)(self.clone());
                  // TODO: wake up tasks if specified.
                  ret |= r;
            }
            ret
      }

      pub fn affinity(&self) -> &super::CpuMask {
            &self.affinity
      }
}

// TODO: Write different types of interrupt handling routines, such as EDGE, LEVEL, 
// FASTEOI, etc.
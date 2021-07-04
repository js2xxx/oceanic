pub mod alloc;
pub mod ctx;
pub(super) mod def;

use self::def::NR_VECTORS;
use crate::cpu::intr::Interrupt;

use ::alloc::sync::{Arc, Weak};
use spin::Mutex;

const VEC_INTR_INIT: Mutex<Option<Weak<Interrupt>>> = Mutex::new(None);
#[thread_local]
static VEC_INTR: [Mutex<Option<Weak<Interrupt>>>; NR_VECTORS] = [VEC_INTR_INIT; NR_VECTORS];

#[derive(Debug, Clone)]
pub struct ArchReg {
      vec: u16,
      cpu: usize,
}

#[derive(Debug)]
pub enum RegisterError {
      NotCurCpu,
      Pending,
}

/// # Safety
///
/// WARNING: This function modifies the architecture's basic registers. Be sure to make
/// preparations.
pub unsafe fn try_register(
      intr: &Arc<Interrupt>,
) -> Result<Option<Weak<Interrupt>>, RegisterError> {
      let ArchReg { vec, cpu } = intr.arch_reg().lock().clone();
      if cpu != crate::cpu::id() {
            return Err(RegisterError::NotCurCpu);
      }

      if let Some(mut intr_slot) = VEC_INTR[vec as usize].try_lock() {
            Ok(intr_slot.replace(Arc::downgrade(intr)))
      } else {
            Err(RegisterError::Pending)
      }
}

/// # Safety
///
/// This function must only be called from its assembly routine `rout_XX`.
#[no_mangle]
unsafe extern "C" fn common_interrupt(frame: *mut ctx::Frame) {
      let vec = unsafe { &*frame }.errc_vec as u16;
      if let Some(mut intr_slot) = VEC_INTR[vec as usize].try_lock() {
            if let Some(intr) = intr_slot.clone().and_then(|intr_weak| {
                  intr_weak.upgrade().or_else(|| {
                        // Automatically unregister the interrupt weak link.
                        let _ = intr_slot.take();
                        None
                  })
            }) {
                  intr.handle();
            } else {
                  let kernel_gs = unsafe { crate::cpu::arch::KernelGs::access_in_intr() };
                  let lapic = &mut kernel_gs.lapic;
                  lapic.eoi();

                  log::warn!("No interrupt for vector {:X}", vec);
            }
      } else {
            log::warn!(
                  "The interrupt for vector {:X} is already firing without block next ones",
                  vec
            );
      }
}

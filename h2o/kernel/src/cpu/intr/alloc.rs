use super::arch::{try_register, try_unregister, RegisterError};
use super::{Interrupt, IntrChip};
use crate::cpu::CpuMask;

use alloc::sync::Arc;
use core::sync::atomic::AtomicU16;
use spin::Mutex;

#[derive(Debug)]
pub enum AllocError {
      ArchReg(super::arch::alloc::AllocError),
      Chip(&'static str),
      Register(RegisterError),
}

pub struct Allocator {
      arch: super::arch::alloc::Allocator,
}

impl Allocator {
      pub fn new(cpu_num: usize) -> Allocator {
            Allocator {
                  arch: super::arch::alloc::Allocator::new(cpu_num),
            }
      }

      pub fn alloc_setup(
            &mut self,
            gsi: u32,
            hw_irq: u8,
            chip: Arc<Mutex<dyn IntrChip>>,
            affinity: CpuMask,
      ) -> Result<Arc<Interrupt>, AllocError> {
            let arch_reg = self.arch.alloc(&affinity).map_err(AllocError::ArchReg)?;

            let handler = unsafe {
                  chip.lock()
                        .setup(arch_reg.clone(), gsi)
                        .map_err(AllocError::Chip)?
            };

            let intr = Arc::new(Interrupt {
                  state: AtomicU16::new(0),
                  gsi,
                  hw_irq,
                  chip,
                  arch_reg: Mutex::new(arch_reg),
                  handler,
                  affinity,
            });

            while match unsafe { try_register(&intr) } {
                  Ok(_) => false,
                  Err(RegisterError::NotCurCpu) => {
                        return Err(AllocError::Register(RegisterError::NotCurCpu))
                  }
                  Err(RegisterError::Pending) => true,
            } {}

            Ok(intr)
      }

      pub fn dealloc_remove(&mut self, intr: Arc<Interrupt>) -> Result<(), AllocError> {
            while match unsafe { try_unregister(&intr) } {
                  Ok(_) => false,
                  Err(RegisterError::NotCurCpu) => {
                        return Err(AllocError::Register(RegisterError::NotCurCpu))
                  }
                  Err(RegisterError::Pending) => true,
            } {}

            unsafe { intr.chip.lock().remove(intr.clone()) }.map_err(AllocError::Chip)?;

            let arch_reg = intr.arch_reg.lock().clone();
            self.arch.dealloc(arch_reg).map_err(AllocError::ArchReg)?;

            Ok(())
      }
}
use super::{Handler, Interrupt};
use crate::cpu::CpuMask;

use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug)]
pub enum AllocError {
      ArchReg(super::arch::alloc::AllocError),
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

      pub fn alloc(
            &mut self,
            gsi: u32,
            hw_irq: u8,
            affinity: CpuMask,
            handler: Vec<Handler>,
      ) -> Result<Arc<Interrupt>, AllocError> {
            let arch_reg = self.arch.alloc(&affinity).map_err(AllocError::ArchReg)?;

            let intr = Arc::new(Interrupt {
                  gsi,
                  hw_irq,
                  arch_reg: Mutex::new(arch_reg.clone()),
                  handler,
                  affinity,
            });

            Ok(intr)
      }

      pub fn dealloc(&mut self, intr: Arc<Interrupt>) -> Result<(), AllocError> {
            let arch_reg = intr.arch_reg.lock().clone();
            self.arch.dealloc(arch_reg).map_err(AllocError::ArchReg)?;

            Ok(())
      }
}

use super::def::NR_VECTORS;
use super::ArchReg;
use crate::cpu::CpuMask;

use alloc::vec::Vec;
use bitvec::prelude::*;
use core::ops::Range;

#[derive(Debug, Clone)]
struct CpuChunk {
      bitmap: BitArr!(for NR_VECTORS),
      free_cnt: usize,
}

pub struct Allocator {
      cpus: Vec<CpuChunk>,
      range: Range<u16>,
}

#[derive(Debug)]
pub enum AllocError {
      Available(bool),
      Range(u16, Range<u16>),
}

impl Allocator {
      pub fn new(cpu_num: usize, range: Range<u16>) -> Self {
            Allocator {
                  cpus: alloc::vec![CpuChunk {
                        bitmap: bitarr![0; NR_VECTORS],
                        free_cnt: (range.end - range.start).into(),
                  }; cpu_num],
                  range,
            }
      }

      pub fn allocable_range(&self) -> Range<u16> {
            self.range.clone()
      }

      fn alloc_idx(&mut self, alloc_cpu: &CpuMask) -> Result<(u16, usize), AllocError> {
            let range = self.allocable_range();

            let cpu = alloc_cpu
                  .iter_ones()
                  .find(|&cpu| self.cpus[cpu].free_cnt > 0)
                  .map_or(Err(AllocError::Available(false)), Ok)?;

            let cpu_chunk = &mut self.cpus[cpu];
            let pos = cpu_chunk
                  .bitmap
                  .iter_zeros()
                  .find(|&pos| range.contains(&(pos as u16)))
                  .expect("CPU's `free_cnt` is not corresponding to its bitmap.");

            cpu_chunk.bitmap.set(pos, true);
            cpu_chunk.free_cnt -= 1;

            Ok((pos as u16, cpu))
      }

      pub fn alloc(&mut self, alloc_cpu: &CpuMask) -> Result<ArchReg, AllocError> {
            self.alloc_idx(alloc_cpu)
                  .map(|(vec, cpu)| ArchReg { vec, cpu })
      }

      fn dealloc_idx(&mut self, vec: u16, cpu: usize) -> Result<(), AllocError> {
            let range = self.allocable_range();
            if !range.contains(&vec) {
                  return Err(AllocError::Range(vec, range));
            }

            let pos = vec as usize;
            let cpu_chunk = &mut self.cpus[cpu];

            if !cpu_chunk.bitmap[pos] {
                  Err(AllocError::Available(true))
            } else {
                  cpu_chunk.bitmap.set(pos, false);
                  cpu_chunk.free_cnt += 1;

                  Ok(())
            }
      }

      pub fn dealloc(&mut self, intr: ArchReg) -> Result<(), AllocError> {
            self.dealloc_idx(intr.vec, intr.cpu)
      }
}

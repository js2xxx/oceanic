//! TODO: Add HPET device support.
#![allow(dead_code)]

use crate::mem::space::{krl, Flags, MemBlock};
use crate::dev::acpi::table::hpet::HpetData;

use core::pin::Pin;

#[repr(usize)]
enum HpetReg {
      Id = 0x000,
      Period = 0x004,
      Config = 0x010,
      Status = 0x020,
      Counter = 0x0f0,
      T0Config = 0x100,
      T0Cmp = 0x108,
      T0Route = 0x110,
      T1Config = 0x120,
      T1Cmp = 0x128,
      T1Route = 0x130,
      T2Config = 0x140,
      T2Cmp = 0x148,
      T2Route = 0x150,
}

impl HpetReg {
      fn tn_config(n: usize) -> HpetReg {
            match n {
                  0 => Self::T0Config,
                  1 => Self::T1Config,
                  2 => Self::T2Config,
                  _ => panic!("HPET only have 3 sets"),
            }
      }
      fn tn_cmp(n: usize) -> HpetReg {
            match n {
                  0 => Self::T0Cmp,
                  1 => Self::T1Cmp,
                  2 => Self::T2Cmp,
                  _ => panic!("HPET only have 3 sets"),
            }
      }
      fn tn_route(n: usize) -> HpetReg {
            match n {
                  0 => Self::T0Route,
                  1 => Self::T1Route,
                  2 => Self::T2Route,
                  _ => panic!("HPET only have 3 sets"),
            }
      }
}

pub struct Hpet<'a> {
      base_ptr: *mut u32,
      memory: Pin<&'a mut [MemBlock]>,

      block_id: u8,
      period_fs: u64,
}

impl<'a> Hpet<'a> {
      unsafe fn read_reg(base_ptr: *const u32, reg: HpetReg) -> u32 {
            base_ptr.add(reg as usize).read_volatile()
      }

      unsafe fn write_reg(base_ptr: *mut u32, reg: HpetReg, val: u32) {
            base_ptr.add(reg as usize).write_volatile(val)
      }

      pub unsafe fn new(data: HpetData) -> Result<Self, &'static str> {
            let HpetData {
                  base: phys,
                  block_id,
            } = data;

            let mut memory = unsafe {
                  krl(|space| {
                        space.alloc_manual(
                              paging::PAGE_LAYOUT,
                              Some(phys),
                              Flags::READABLE | Flags::WRITABLE,
                        )
                        .map_err(|_| "Memory allocation failed")
                  })
            }
            .expect("Kernel space uninitialized")?;

            let base_ptr = memory.as_mut_ptr().cast::<u32>();
            let period_fs = Self::read_reg(base_ptr, HpetReg::Period).into();

            Ok(Hpet {
                  base_ptr,
                  memory,
                  block_id,
                  period_fs,
            })
      }
}

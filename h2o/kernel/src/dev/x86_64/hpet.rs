//! TODO: Add HPET device support.
#![allow(dead_code)]

use crate::mem::space::MemBlock;

use core::pin::Pin;

const HPET_ID: usize = 0x000;
const HPET_PERIOD: usize = 0x004;
const HPET_CFG: usize = 0x010;
const HPET_STATUS: usize = 0x020;
const HPET_COUNTER: usize = 0x0f0;
const HPET_TN_CFG: [usize; 3] = [0x100, 0x120, 0x140];
const HPET_TN_CMP: [usize; 3] = [0x108, 0x128, 0x148];
const HPET_TN_ROUTE: [usize; 3] = [0x110, 0x120, 0x140];

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
}

impl<'a> Hpet<'a> {
      unsafe fn read_reg(&self, reg: HpetReg) -> u32 {
            self.base_ptr.add(reg as usize).read_volatile()
      }

      unsafe fn write_reg(&mut self, reg: HpetReg, val: u32) {
            self.base_ptr.add(reg as usize).write_volatile(val)
      }
}

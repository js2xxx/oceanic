use crate::cpu::intr::{Interrupt, IntrChip};
use crate::mem::space;
use archop::msr;

use alloc::sync::Arc;
use core::pin::Pin;

const LAPIC_LAYOUT: core::alloc::Layout = paging::PAGE_LAYOUT;

pub enum LapicType<'a> {
      X1(Pin<&'a mut [space::MemBlock]>),
      X2,
}

pub struct Lapic<'a> {
      ty: LapicType<'a>,
      id: u32,
}

impl<'a> Lapic<'a> {
      fn reg_32_to_1_off(reg: msr::Msr) -> usize {
            (reg as u32 as usize - 0x800) << 4
      }

      fn reg_64_to_1_off(reg: msr::Msr) -> [usize; 2] {
            let r0 = Self::reg_32_to_1_off(reg);
            [r0, r0 + 0x10]
      }

      unsafe fn read_reg_32(ty: &mut LapicType, reg: msr::Msr) -> u32 {
            match ty {
                  LapicType::X1(memory) => {
                        let base = memory.as_ptr().cast::<u8>();
                        let ptr = base.add(Self::reg_32_to_1_off(reg)).cast::<u32>();
                        ptr.read_volatile()
                  }
                  LapicType::X2 => msr::read(reg) as u32,
            }
      }

      unsafe fn write_reg_32(ty: &mut LapicType, reg: msr::Msr, val: u32) {
            match ty {
                  LapicType::X1(memory) => {
                        let base = memory.as_mut_ptr().cast::<u8>();
                        let ptr = base.add(Self::reg_32_to_1_off(reg)).cast::<u32>();
                        ptr.write_volatile(val)
                  }
                  LapicType::X2 => msr::write(reg, val as u64),
            }
      }

      unsafe fn read_reg_64(ty: &mut LapicType, reg: msr::Msr) -> u64 {
            match ty {
                  LapicType::X1(memory) => {
                        let base = memory.as_ptr().cast::<u8>();

                        let ptr_array = Self::reg_64_to_1_off(reg);
                        let mut ptr_iter = ptr_array.iter().map(|&off| base.add(off).cast::<u32>());
                        let low = ptr_iter.next().unwrap().read_volatile() as u64;
                        let high = ptr_iter.next().unwrap().read_volatile() as u64;
                        low | (high << 32)
                  }
                  LapicType::X2 => msr::read(reg),
            }
      }

      unsafe fn write_reg_64(ty: &mut LapicType, reg: msr::Msr, val: u64) {
            match ty {
                  LapicType::X1(memory) => {
                        let base = memory.as_mut_ptr().cast::<u8>();
                        let (low, high) = ((val & 0xFFFFFFFF) as u32, ((val >> 32) as u32));

                        let ptr_array = Self::reg_64_to_1_off(reg);
                        let mut ptr_iter = ptr_array
                              .iter()
                              .map(|&off| base.add(off).cast::<u32>())
                              .rev(); // !!: The order of writing must be from high to low.
                        ptr_iter.next().unwrap().write_volatile(high);
                        ptr_iter.next().unwrap().write_volatile(low);
                  }
                  LapicType::X2 => msr::write(reg, val),
            }
      }

      pub fn new(ty: acpi::table::madt::LapicType, space: &'a Arc<space::Space>) -> Self {
            let mut ty = match ty {
                  acpi::table::madt::LapicType::X2 => {
                        // SAFE: Enabling Local X2 APIC if possible.
                        unsafe {
                              let val = msr::read(msr::APIC_BASE);
                              msr::write(msr::APIC_BASE, val | (1 << 10));
                        }
                        LapicType::X2
                  }
                  acpi::table::madt::LapicType::X1(paddr) => {
                        // SAFE: The physical address is valid and aligned.
                        let memory = unsafe {
                              space.alloc_manual(
                                    LAPIC_LAYOUT,
                                    Some(paddr),
                                    false,
                                    space::Flags::READABLE | space::Flags::WRITABLE,
                              )
                        }
                        .expect("Failed to allocate space");
                        LapicType::X1(memory)
                  }
            };

            let mut id = unsafe { Self::read_reg_32(&mut ty, msr::X2APICID) };
            if let LapicType::X2 = &ty {
                  id >>= 24;
            }

            Lapic { ty, id }
      }

      /// # Safety
      ///
      /// WARNING: This function modifies the architecture's basic registers. Be sure to make
      /// preparations.
      pub unsafe fn enable(&mut self) {
            Self::write_reg_32(
                  &mut self.ty,
                  msr::X2APIC_SIVR,
                  (1 << 8) | (super::intr::def::ApicVec::Spurious as u32),
            );
      }

      pub fn id(&self) -> u32 {
            self.id
      }
}

impl<'a> IntrChip for Lapic<'a> {
      unsafe fn mask(&mut self, intr: Arc<Interrupt>) {
            todo!()
      }

      unsafe fn unmask(&mut self, intr: Arc<Interrupt>) {
            todo!()
      }

      unsafe fn ack(&mut self, intr: Arc<Interrupt>) {
            todo!()
      }

      unsafe fn eoi(&mut self, intr: Arc<Interrupt>) {
            Self::write_reg_32(&mut self.ty, msr::X2APIC_EOI, 0)
      }
}

use crate::cpu::intr::{Interrupt, IntrChip};
use crate::mem::space::{Flags, MemBlock, Space};
use acpi::table::madt::ioapic::{IntrOvrPolarity, IntrOvrTrig};
use acpi::table::madt::{IoapicData, IoapicNode};
use paging::{PAddr, PAGE_LAYOUT, PAGE_MASK};

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::Range;
use core::pin::Pin;
use modular_bitfield::prelude::*;

const LEGACY_IRQ: Range<u32> = 0..16;

#[derive(Debug, Copy, Clone)]
enum IoapicReg {
      IoapicId,
      IoapicVer,
      IoapicArb,
      IoRedirTable(u8),
}

impl From<IoapicReg> for u32 {
      fn from(reg: IoapicReg) -> Self {
            match reg {
                  IoapicReg::IoapicId => 0,
                  IoapicReg::IoapicVer => 1,
                  IoapicReg::IoapicArb => 2,
                  IoapicReg::IoRedirTable(pin) => 0x10 + pin as u32 * 2,
            }
      }
}

#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
#[repr(u64)]
#[bits = 3]
pub enum DelivMode {
      Fixed = 0b000,
      LowestPriority = 0b001,
      Smi = 0b010,
      Nmi = 0b100,
      Init = 0b101,
      ExtInt = 0b111,
}

#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
#[repr(u64)]
pub enum Polarity {
      High = 0,
      Low = 1,
}

#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
#[repr(u64)]
pub enum TriggerMode {
      Edge = 0,
      Level = 1,
}

#[derive(Clone, Copy)]
#[bitfield]
struct IoapicEntry {
      vec: u8,
      #[bits = 3]
      deliv_mode: DelivMode,
      dest_logical: bool,
      pending: bool,
      #[bits = 1]
      polarity: Polarity,
      remote_irr: bool,
      #[bits = 1]
      trigger_mode: TriggerMode,
      mask: bool,
      #[skip]
      __: B32,
      dest_hi: B7,
      dest: u8,
}

impl From<u64> for IoapicEntry {
      fn from(x: u64) -> Self {
            Self::from_bytes(x.to_ne_bytes())
      }
}

impl From<IoapicEntry> for u64 {
      fn from(x: IoapicEntry) -> Self {
            Self::from_ne_bytes(x.into_bytes())
      }
}

unsafe fn write_regsel(base_ptr: *mut u32, val: u32) {
      base_ptr.write_volatile(val);
}

unsafe fn read_win(base_ptr: *const u32) -> u32 {
      base_ptr.add(4).read_volatile()
}

unsafe fn write_win(base_ptr: *mut u32, val: u32) {
      base_ptr.add(4).write_volatile(val);
}

unsafe fn write_eoi(base_ptr: *mut u32, val: u32) {
      base_ptr.add(16).write_volatile(val);
}

pub struct Ioapic<'a> {
      memory: Pin<&'a mut [MemBlock]>,
      base_ptr: *mut u32,
      id: u8,
      version: u32,
      gsi: Range<u32>,
}

impl<'a> Ioapic<'a> {
      unsafe fn read_reg(&mut self, reg: u32) -> u32 {
            write_regsel(self.base_ptr, reg);
            read_win(self.base_ptr)
      }

      unsafe fn write_reg(&mut self, reg: u32, val: u32) {
            write_regsel(self.base_ptr, reg);
            write_win(self.base_ptr, val);
      }

      unsafe fn read_ioredtbl(&mut self, pin: u8) -> u64 {
            let reg: u32 = IoapicReg::IoRedirTable(pin).into();
            self.read_reg(reg) as u64 | (self.read_reg(reg + 1) as u64) << 32
      }

      unsafe fn write_ioredtbl(&mut self, pin: u8, val: u64) {
            let reg: u32 = IoapicReg::IoRedirTable(pin).into();
            // Higher DWORD first, for the mask bit is in the lower DWORD.
            self.write_reg(reg + 1, (val >> 32) as u32);
            self.write_reg(reg, (val & 0xFFFFFFFF) as u32);
      }

      /// # Safety
      ///
      /// The caller must ensure that this function is called only once per I/O APIC ID.
      pub unsafe fn new(space: &'a Arc<Space>, node: IoapicNode) -> Result<Self, &'static str> {
            let IoapicNode {
                  id,
                  paddr,
                  gsi_base,
            } = node;

            let (base, offset) = {
                  let paddr = PAddr::new(paddr as usize);
                  (PAddr::new(*paddr & !PAGE_MASK), paddr.in_page_offset())
            };
            let mut memory = unsafe {
                  space.alloc_manual(
                        PAGE_LAYOUT,
                        Some(base),
                        false,
                        Flags::READABLE | Flags::WRITABLE,
                  )
                  .map_err(|_| "Memory allocation failed")?
            };
            let base_ptr = unsafe { memory.as_mut_ptr().cast::<u8>().add(offset) }.cast::<u32>();

            let mut ioapic = Ioapic {
                  memory,
                  base_ptr,
                  id,
                  version: 0,
                  gsi: 0..0,
            };
            let (version, size) = {
                  let val = ioapic.read_reg(IoapicReg::IoapicVer.into());
                  (val & 0xFF, ((val >> 16) & 0xFF) + 1)
            };
            ioapic.version = version;
            ioapic.gsi = gsi_base..(gsi_base + size);

            Ok(ioapic)
      }

      pub fn size(&self) -> usize {
            self.gsi.len()
      }
}

struct IntrOvr {
      hw_irq: u8,
      gsi: u32,
      polarity: Polarity,
      trigger_mode: TriggerMode,
}

pub struct Ioapics<'a> {
      ioapic_data: Vec<Ioapic<'a>>,
      intr_ovr: Vec<IntrOvr>,
}

impl<'a> Ioapics<'a> {
      pub unsafe fn new(space: &'a Arc<Space>, ioapic_data: IoapicData) -> Self {
            let IoapicData {
                  ioapic: acpi_ioapics,
                  intr_ovr: acpi_intr_ovr,
            } = ioapic_data;

            let mut ioapic_data = Vec::new();
            for acpi_ioapic in acpi_ioapics {
                  if let Ok(ioapic) = Ioapic::new(space, acpi_ioapic) {
                        ioapic_data.push(ioapic);
                  }
            }

            let mut intr_ovr = Vec::new();
            for acpi_io in acpi_intr_ovr {
                  let gsi = acpi_io.gsi;
                  let hw_irq = acpi_io.hw_irq;

                  let isa = LEGACY_IRQ.contains(&gsi);
                  if isa && gsi != hw_irq.into() {
                        continue;
                  }

                  let polarity = match acpi_io.polarity {
                        IntrOvrPolarity::None => {
                              if isa {
                                    Polarity::High
                              } else {
                                    Polarity::Low
                              }
                        }
                        IntrOvrPolarity::High => Polarity::High,
                        IntrOvrPolarity::Low => Polarity::Low,
                  };
                  let trigger_mode = match acpi_io.trigger_mode {
                        IntrOvrTrig::None => {
                              if isa {
                                    TriggerMode::Edge
                              } else {
                                    TriggerMode::Level
                              }
                        }
                        IntrOvrTrig::Edge => TriggerMode::Edge,
                        IntrOvrTrig::Level => TriggerMode::Level,
                  };

                  intr_ovr.push(IntrOvr {
                        hw_irq,
                        gsi,
                        polarity,
                        trigger_mode,
                  });
            }

            Ioapics {
                  ioapic_data,
                  intr_ovr,
            }
      }

      pub fn chip_pin(&self, gsi: u32) -> Option<(&Ioapic<'_>, u8)> {
            for chip in self.ioapic_data.iter() {
                  if chip.gsi.contains(&gsi) {
                        return Some((chip, (gsi - chip.gsi.start) as u8));
                  }
            }
            None
      }

      pub fn chip_mut_pin(&mut self, gsi: u32) -> Option<(&mut Ioapic<'a>, u8)> {
            for chip in self.ioapic_data.iter_mut() {
                  if chip.gsi.contains(&gsi) {
                        return Some((chip, (gsi - chip.gsi.start) as u8));
                  }
            }
            None
      }
}

impl<'a> IntrChip for Ioapics<'a> {
      unsafe fn mask(&mut self, intr: Arc<Interrupt>) {
            let gsi = intr.gsi();
            let (chip, pin) = match self.chip_mut_pin(gsi) {
                  Some(res) => res,
                  None => return,
            };

            let entry = IoapicEntry::from(chip.read_ioredtbl(pin));
            entry.set_mask(true);
            chip.write_ioredtbl(pin, entry.into());

            // todo!("Add chip data trait (downcasting to `IoapicEntry`) for interrupts")
      }

      unsafe fn unmask(&mut self, intr: Arc<Interrupt>) {
            let gsi = intr.gsi();
            let (chip, pin) = match self.chip_mut_pin(gsi) {
                  Some(res) => res,
                  None => return,
            };

            let entry = IoapicEntry::from(chip.read_ioredtbl(pin));
            entry.set_mask(false);
            chip.write_ioredtbl(pin, entry.into());
      }

      unsafe fn ack(&mut self, _intr: Arc<Interrupt>) {}

      unsafe fn eoi(&mut self, intr: Arc<Interrupt>) {
            let gsi = intr.gsi();
            let (chip, pin) = match self.chip_mut_pin(gsi) {
                  Some(res) => res,
                  None => return,
            };

            let entry = IoapicEntry::from(chip.read_ioredtbl(pin));
            if chip.version >= 0x20 {
                  write_eoi(chip.base_ptr, entry.vec().into());
            } else {
                  // Manually mask and unmask the entry to refresh the state.
                  let mut cloned = entry;
                  cloned.set_mask(true);
                  chip.write_ioredtbl(pin, cloned.into());
                  chip.write_ioredtbl(pin, entry.into());
            }
      }
}

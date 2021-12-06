use alloc::{sync::Arc, vec::Vec};
use core::ops::Range;

use acpi::platform::interrupt::{
    Apic, IoApic as AcpiIoapic, Polarity as AcpiPolarity, TriggerMode as AcpiTriggerMode,
};
use modular_bitfield::prelude::*;
use paging::{PAddr, PAGE_LAYOUT};
use spin::{Lazy, Mutex};

use crate::{
    cpu::{
        arch::{
            apic::{lapic, DelivMode, Polarity, TriggerMode, LAPIC_ID},
            intr::ArchReg,
        },
        intr::{edge_handler, fasteoi_handler, Interrupt, IntrChip, TypeHandler},
    },
    mem::space::{self, AllocType, Flags, KernelVirt, Phys},
};

const LEGACY_IRQ: Range<u32> = 0..16;

static IOAPIC_CHIP: Lazy<(Arc<Mutex<dyn IntrChip>>, Vec<IntrOvr>)> = Lazy::new(|| unsafe {
    let ioapic_data = match crate::dev::acpi::platform_info().interrupt_model {
        acpi::InterruptModel::Apic(ref apic) => apic,
        _ => panic!("Failed to get IOAPIC data"),
    };
    let (ioapics, intr_ovr) = Ioapics::new(ioapic_data);
    (Arc::new(Mutex::new(ioapics)), intr_ovr)
});

pub fn chip() -> Arc<Mutex<dyn IntrChip>> {
    IOAPIC_CHIP.0.clone()
}

fn intr_ovr() -> &'static [IntrOvr] {
    &IOAPIC_CHIP.1
}

pub fn gsi_from_isa(irq: crate::cpu::intr::IsaIrq) -> u32 {
    let raw = irq as u8;
    for intr_ovr in intr_ovr() {
        if intr_ovr.hw_irq == raw {
            return intr_ovr.gsi;
        }
    }
    u32::from(raw)
}

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

pub struct Ioapic {
    base_ptr: *mut u32,
    virt: KernelVirt,

    id: u8,
    version: u32,
    gsi: Range<u32>,
}

unsafe impl Send for Ioapic {}
unsafe impl Sync for Ioapic {}

impl Ioapic {
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
        self.read_reg(reg) as u64 | ((self.read_reg(reg + 1) as u64) << 32)
    }

    unsafe fn write_ioredtbl(&mut self, pin: u8, val: u64) {
        let reg: u32 = IoapicReg::IoRedirTable(pin).into();
        // Higher DWORD first, for the mask bit is in the lower DWORD.
        self.write_reg(reg + 1, (val >> 32) as u32);
        self.write_reg(reg, (val & 0xFFFFFFFF) as u32);
    }

    /// # Safety
    ///
    /// The caller must ensure that this function is called only once per I/O
    /// APIC ID.
    pub unsafe fn new(node: &AcpiIoapic) -> Self {
        let AcpiIoapic {
            id,
            address: paddr,
            global_system_interrupt_base: gsi_base,
        } = node;
        let phys = Phys::new(
            PAddr::new(*paddr as usize),
            PAGE_LAYOUT,
            Flags::READABLE | Flags::WRITABLE | Flags::UNCACHED,
        );
        let virt = unsafe {
            space::current()
                .allocate_kernel(
                    AllocType::Layout(phys.layout()),
                    Some(phys.clone()),
                    phys.flags(),
                )
                .expect("Failed to allocate memory")
        };
        let base_ptr = virt.as_ptr().cast::<u32>().as_ptr();
        let mut ioapic = Ioapic {
            base_ptr,
            virt,
            id: *id,
            version: 0,
            gsi: 0..0,
        };
        let (version, size) = {
            let val = ioapic.read_reg(IoapicReg::IoapicVer.into());
            (val & 0xFF, ((val >> 16) & 0xFF) + 1)
        };
        ioapic.version = version;
        ioapic.gsi = *gsi_base..(*gsi_base + size);

        ioapic
    }

    pub fn size(&self) -> usize {
        self.gsi.len()
    }

    pub fn virt(&self) -> &KernelVirt {
        &self.virt
    }
}

struct IntrOvr {
    hw_irq: u8,
    gsi: u32,
    polarity: Polarity,
    trigger_mode: TriggerMode,
}

pub struct Ioapics {
    ioapic_data: Vec<Ioapic>,
}

impl Ioapics {
    unsafe fn new(ioapic_data: &Apic) -> (Self, Vec<IntrOvr>) {
        let Apic {
            io_apics: acpi_ioapics,
            interrupt_source_overrides: acpi_intr_ovr,
            ..
        } = ioapic_data;

        let ioapic_data = acpi_ioapics
            .iter()
            .map(|data| Ioapic::new(data))
            .collect::<Vec<_>>();

        let intr_ovr = acpi_intr_ovr
            .iter()
            .map(|acpi_io| {
                let gsi = acpi_io.global_system_interrupt;
                let hw_irq = acpi_io.isa_source;
                let isa = LEGACY_IRQ.contains(&gsi);
                IntrOvr {
                    hw_irq,
                    gsi,
                    polarity: match acpi_io.polarity {
                        AcpiPolarity::SameAsBus => {
                            if isa {
                                Polarity::High
                            } else {
                                Polarity::Low
                            }
                        }
                        AcpiPolarity::ActiveHigh => Polarity::High,
                        AcpiPolarity::ActiveLow => Polarity::Low,
                    },
                    trigger_mode: match acpi_io.trigger_mode {
                        AcpiTriggerMode::SameAsBus => {
                            if isa {
                                TriggerMode::Edge
                            } else {
                                TriggerMode::Level
                            }
                        }
                        AcpiTriggerMode::Edge => TriggerMode::Edge,
                        AcpiTriggerMode::Level => TriggerMode::Level,
                    },
                }
            })
            .collect::<Vec<_>>();

        (Ioapics { ioapic_data }, intr_ovr)
    }

    pub fn chip_pin(&self, gsi: u32) -> Option<(&Ioapic, u8)> {
        for chip in self.ioapic_data.iter() {
            if chip.gsi.contains(&gsi) {
                return Some((chip, (gsi - chip.gsi.start) as u8));
            }
        }
        None
    }

    pub fn chip_mut_pin(&mut self, gsi: u32) -> Option<(&mut Ioapic, u8)> {
        for chip in self.ioapic_data.iter_mut() {
            if chip.gsi.contains(&gsi) {
                let start = chip.gsi.start;
                return Some((chip, (gsi - start) as u8));
            }
        }
        None
    }
}

impl IntrChip for Ioapics {
    unsafe fn setup(&mut self, arch_reg: ArchReg, gsi: u32) -> Result<TypeHandler, &'static str> {
        let (vec, apic_id) = {
            (
                arch_reg.vector(),
                *LAPIC_ID
                    .read()
                    .get(&arch_reg.cpu())
                    .ok_or("Failed to get LAPIC ID")?,
            )
        };
        let (trigger_mode, polarity) =
            if let Some(intr_ovr) = intr_ovr().iter().find(|i| i.gsi == gsi) {
                (intr_ovr.trigger_mode, intr_ovr.polarity)
            } else if LEGACY_IRQ.contains(&gsi) {
                (TriggerMode::Edge, Polarity::High)
            } else {
                (TriggerMode::Level, Polarity::Low)
            };

        let (chip, pin) = self
            .chip_mut_pin(gsi)
            .ok_or("Failed to find a chip for the GSI")?;

        let entry = IoapicEntry::new()
            .with_vec(vec)
            .with_deliv_mode(DelivMode::Fixed)
            .with_trigger_mode(trigger_mode)
            .with_polarity(polarity)
            .with_mask(true)
            .with_dest((apic_id & 0xFF) as u8)
            .with_dest_hi(((apic_id >> 8) & 0xFF) as u8);

        chip.write_ioredtbl(pin, entry.into());

        Ok(match trigger_mode {
            TriggerMode::Edge => edge_handler,
            TriggerMode::Level => fasteoi_handler,
        })
    }

    unsafe fn remove(&mut self, intr: Arc<Interrupt>) -> Result<(), &'static str> {
        let gsi = intr.gsi();
        let (chip, pin) = self
            .chip_mut_pin(gsi)
            .ok_or("Failed to find a chip for the GSI")?;

        let entry = IoapicEntry::new().with_mask(true);
        chip.write_ioredtbl(pin, entry.into());

        Ok(())
    }

    unsafe fn mask(&mut self, intr: Arc<Interrupt>) {
        let gsi = intr.gsi();
        let (chip, pin) = match self.chip_mut_pin(gsi) {
            Some(res) => res,
            None => return,
        };

        let mut entry = IoapicEntry::from(chip.read_ioredtbl(pin));
        entry.set_mask(true);
        chip.write_ioredtbl(pin, entry.into());
    }

    unsafe fn unmask(&mut self, intr: Arc<Interrupt>) {
        let gsi = intr.gsi();
        let (chip, pin) = match self.chip_mut_pin(gsi) {
            Some(res) => res,
            None => return,
        };

        let mut entry = IoapicEntry::from(chip.read_ioredtbl(pin));
        entry.set_mask(false);
        chip.write_ioredtbl(pin, entry.into());
    }

    unsafe fn ack(&mut self, _intr: Arc<Interrupt>) {
        lapic(|lapic| lapic.eoi());
    }

    unsafe fn eoi(&mut self, intr: Arc<Interrupt>) {
        lapic(|lapic| lapic.eoi());

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

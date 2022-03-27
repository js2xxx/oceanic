use alloc::vec::Vec;
use core::ops::Range;

use acpi::platform::interrupt::{
    Apic, InterruptSourceOverride as AcpiIntrOvr, IoApic as AcpiIoapic, Polarity as AcpiPolarity,
    TriggerMode as AcpiTriggerMode,
};
use archop::Azy;
use collection_ex::RangeMap;
use modular_bitfield::prelude::*;
use paging::{PAddr, PAGE_SIZE};
use spin::Mutex;

use crate::{
    cpu::arch::apic::{lapic, DelivMode, Polarity, TriggerMode},
    mem::space::{self, Flags, Phys},
};

const LEGACY_IRQ: Range<u32> = 0..16;

static IOAPIC_CHIP: Azy<(Mutex<Ioapics>, Vec<IntrOvr>)> = Azy::new(|| {
    let ioapic_data = match crate::dev::acpi::platform_info().interrupt_model {
        acpi::InterruptModel::Apic(ref apic) => apic,
        _ => panic!("Failed to get IOAPIC data"),
    };
    let (ioapics, intr_ovr) =
        unsafe { Ioapics::new(ioapic_data) }.expect("Failed to create IOAPIC");
    (Mutex::new(ioapics), intr_ovr)
});

#[inline]
pub fn chip() -> &'static Mutex<Ioapics> {
    &IOAPIC_CHIP.0
}

#[inline]
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

#[allow(dead_code)]
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
#[must_use]
pub struct IoapicEntry {
    pub vec: u8,
    #[skip(getters)]
    #[bits = 3]
    deliv_mode: DelivMode,
    #[skip(getters)]
    dest_logical: bool,
    #[skip(getters)]
    pending: bool,
    #[bits = 1]
    pub polarity: Polarity,
    #[skip(getters)]
    remote_irr: bool,
    #[bits = 1]
    pub trigger_mode: TriggerMode,
    pub mask: bool,
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

impl IoapicEntry {
    #[inline]
    pub fn dest_id(&self) -> u32 {
        (self.dest() as u32) | ((self.dest_hi() as u32) << 8)
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

    id: u8,
    version: u32,
    gsi: Range<u32>,
}

// [`Ioapic`] lives in the kernel space and should share its data.
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
        let phys = Phys::new(PAddr::new(*paddr as usize), PAGE_SIZE)
            .expect("Failed to acquire memory for I/O APIC");
        let addr = space::KRL
            .map(
                None,
                Phys::clone(&phys),
                0,
                space::page_aligned(phys.len()),
                Flags::READABLE | Flags::WRITABLE | Flags::UNCACHED,
            )
            .expect("Failed to allocate memory");
        let base_ptr = addr.cast::<u32>();
        let mut ioapic = Ioapic {
            base_ptr,
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

    pub fn id(&self) -> u8 {
        self.id
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

impl IntrOvr {
    fn new(acpi_io: &AcpiIntrOvr) -> Self {
        let gsi = acpi_io.global_system_interrupt;
        let hw_irq = acpi_io.isa_source;
        let isa = LEGACY_IRQ.contains(&gsi);
        let polarity = match acpi_io.polarity {
            AcpiPolarity::SameAsBus => {
                if isa {
                    Polarity::High
                } else {
                    Polarity::Low
                }
            }
            AcpiPolarity::ActiveHigh => Polarity::High,
            AcpiPolarity::ActiveLow => Polarity::Low,
        };
        let trigger_mode = match acpi_io.trigger_mode {
            AcpiTriggerMode::SameAsBus => {
                if isa {
                    TriggerMode::Edge
                } else {
                    TriggerMode::Level
                }
            }
            AcpiTriggerMode::Edge => TriggerMode::Edge,
            AcpiTriggerMode::Level => TriggerMode::Level,
        };
        IntrOvr {
            hw_irq,
            gsi,
            polarity,
            trigger_mode,
        }
    }
}

pub struct Ioapics {
    ioapic_data: RangeMap<u32, Ioapic>,
}

impl Ioapics {
    unsafe fn new(ioapic_data: &Apic) -> sv_call::Result<(Self, Vec<IntrOvr>)> {
        let Apic {
            io_apics: acpi_ioapics,
            interrupt_source_overrides: acpi_intr_ovr,
            ..
        } = ioapic_data;

        let mut ioapic_data = RangeMap::new(0..u32::MAX);
        for acpi_ioapic in acpi_ioapics {
            let ioapic = Ioapic::new(acpi_ioapic);
            ioapic_data.try_insert_with(
                ioapic.gsi.clone(),
                || Ok::<_, sv_call::Error>((ioapic, ())),
                sv_call::Error::EEXIST,
            )?;
        }

        let intr_ovr = acpi_intr_ovr.iter().map(IntrOvr::new).collect::<Vec<_>>();

        Ok((Ioapics { ioapic_data }, intr_ovr))
    }

    #[inline]
    fn chip_mut_pin(&mut self, gsi: u32) -> Option<(&mut Ioapic, u8)> {
        self.ioapic_data
            .get_contained_mut(&gsi)
            .map(|(range, chip)| (chip, (gsi - range.start) as u8))
    }
}

impl Ioapics {
    pub fn gsi_range(&self) -> Option<Range<u32>> {
        let (first, _) = self.ioapic_data.first()?;
        let (last, _) = self.ioapic_data.last()?;
        Some(first.start..last.end)
    }

    /// After the call, the entry is masked and must be manually unmasked if
    /// necessary.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the entry corresponding to `gsi` is not used
    /// by others.
    pub unsafe fn config_dest(&mut self, gsi: u32, vec: u8, apic_id: u32) -> sv_call::Result {
        let (chip, pin) = self.chip_mut_pin(gsi).ok_or(sv_call::Error::EINVAL)?;

        let mut entry = IoapicEntry::from(chip.read_ioredtbl(pin));
        entry.set_vec(vec);
        entry.set_deliv_mode(DelivMode::Fixed);
        entry.set_mask(true);
        entry.set_dest((apic_id & 0xFF) as u8);
        entry.set_dest_hi(((apic_id >> 8) & 0xFF) as u8);
        chip.write_ioredtbl(pin, entry.into());
        Ok(())
    }

    /// After the call, the entry is masked and must be manually unmasked if
    /// necessary.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the entry corresponding to `gsi` is not used
    /// by others.
    pub unsafe fn config(
        &mut self,
        gsi: u32,
        trig_mode: TriggerMode,
        polarity: Polarity,
    ) -> sv_call::Result {
        let (chip, pin) = self.chip_mut_pin(gsi).ok_or(sv_call::Error::EINVAL)?;

        let (t, p) = if let Some(intr_ovr) = intr_ovr().iter().find(|i| i.gsi == gsi) {
            (intr_ovr.trigger_mode, intr_ovr.polarity)
        } else if LEGACY_IRQ.contains(&gsi) {
            (TriggerMode::Edge, Polarity::High)
        } else {
            (trig_mode, polarity)
        };
        if t != trig_mode || p != polarity {
            return Err(sv_call::Error::EPERM);
        }

        let mut entry = IoapicEntry::from(chip.read_ioredtbl(pin));
        entry.set_trigger_mode(t);
        entry.set_polarity(p);
        entry.set_mask(true);
        chip.write_ioredtbl(pin, entry.into());

        Ok(())
    }

    pub fn get_entry(&mut self, gsi: u32) -> sv_call::Result<IoapicEntry> {
        let (chip, pin) = self.chip_mut_pin(gsi).ok_or(sv_call::Error::EINVAL)?;
        Ok(IoapicEntry::from(unsafe { chip.read_ioredtbl(pin) }))
    }

    /// # Safety
    ///
    /// The caller must ensure that the entry corresponding to `gsi` is not used
    /// anymore.
    pub unsafe fn deconfig(&mut self, gsi: u32) -> sv_call::Result {
        let (chip, pin) = self.chip_mut_pin(gsi).ok_or(sv_call::Error::EINVAL)?;

        let entry = IoapicEntry::new().with_mask(true);
        chip.write_ioredtbl(pin, entry.into());

        Ok(())
    }

    /// # Safety
    ///
    /// The caller must ensure that the entry corresponding to `gsi` is not used
    /// by others.
    pub unsafe fn mask(&mut self, gsi: u32, masked: bool) -> sv_call::Result {
        let (chip, pin) = self.chip_mut_pin(gsi).ok_or(sv_call::Error::EINVAL)?;

        let mut entry = IoapicEntry::from(chip.read_ioredtbl(pin));
        entry.set_mask(masked);
        chip.write_ioredtbl(pin, entry.into());

        Ok(())
    }

    /// # Safety
    ///
    /// The caller must ensure that the entry corresponding to `gsi` is not used
    /// by others.
    pub unsafe fn eoi(&mut self, gsi: u32) -> sv_call::Result {
        lapic(|lapic| lapic.eoi());

        let (chip, pin) = self.chip_mut_pin(gsi).ok_or(sv_call::Error::EINVAL)?;

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
        Ok(())
    }
}

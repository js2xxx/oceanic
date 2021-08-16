use crate::raw;
use crate::subt_parser;

use alloc::{vec, vec::Vec};
use core::ptr::null_mut;
use core::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone)]
pub struct IoapicNode {
      pub id: u8,
      pub paddr: u32,
      pub gsi_base: u32,
}

#[derive(Debug, Clone)]
pub enum IntrOvrPolarity {
      None,
      High,
      Low,
}

#[derive(Debug, Clone)]
pub enum IntrOvrTrig {
      None,
      Edge,
      Level,
}

#[derive(Debug, Clone)]
pub struct IntrOvr {
      pub hw_irq: u8,
      pub gsi: u32,
      pub polarity: IntrOvrPolarity,
      pub trigger_mode: IntrOvrTrig,
}

#[derive(Debug, Clone)]
pub struct IoapicData {
      pub ioapic: Vec<IoapicNode>,
      pub intr_ovr: Vec<IntrOvr>,
}

static mut IOAPIC: Vec<IoapicNode> = Vec::new();
static mut INTR_OVR: Vec<IntrOvr> = Vec::new();
static INIT: AtomicBool = AtomicBool::new(false);

/// # Safety
///
/// The caller must ensure that the memory mapping for ACPI tables is fixed and valid,
/// and that this function is not called twice or after SMP initialization.
pub unsafe fn get_ioapic_data() -> Result<IoapicData, raw::ACPI_STATUS> {
      if !INIT.swap(true, Ordering::SeqCst) {
            acquire_ioapic_data()?;
      }

      Ok(IoapicData {
            ioapic: IOAPIC.clone(),
            intr_ovr: INTR_OVR.clone(),
      })
}

unsafe fn acquire_ioapic_data() -> Result<(), raw::ACPI_STATUS> {
      let madt = {
            let mut tbl = null_mut();
            let status = raw::AcpiGetTable(raw::ACPI_SIG_MADT.as_ptr() as _, 0, &mut tbl);
            if status != raw::AE_OK {
                  return Err(status);
            }
            tbl.cast()
      };

      let parser = vec![
            subt_parser!(raw::ACPI_MADT_TYPE_IO_APIC => box |subt| {
                  let ioapic = subt.cast::<raw::ACPI_MADT_IO_APIC>();
                  IOAPIC.push(IoapicNode {
                        id: (*ioapic).Id,
                        paddr: (*ioapic).Address,
                        gsi_base: (*ioapic).GlobalIrqBase,
                  });
            }),
            subt_parser!(raw::ACPI_MADT_TYPE_INTERRUPT_OVERRIDE => box |subt| {
                  let intr_ovr = subt.cast::<raw::ACPI_MADT_INTERRUPT_OVERRIDE>();
                  let polarity = match (*intr_ovr).IntiFlags as u32 & raw::ACPI_MADT_POLARITY_MASK {
                        raw::ACPI_MADT_POLARITY_ACTIVE_HIGH => IntrOvrPolarity::High,
                        raw::ACPI_MADT_POLARITY_ACTIVE_LOW => IntrOvrPolarity::Low,
                        _ => IntrOvrPolarity::None,
                  };
                  let trigger_mode = match (*intr_ovr).IntiFlags as u32 & raw::ACPI_MADT_TRIGGER_MASK {
                        raw::ACPI_MADT_TRIGGER_EDGE => IntrOvrTrig::Edge,
                        raw::ACPI_MADT_TRIGGER_LEVEL => IntrOvrTrig::Level,
                        _ => IntrOvrTrig::None,
                  };
                  INTR_OVR.push(IntrOvr {
                        hw_irq: (*intr_ovr).SourceIrq,
                        gsi: (*intr_ovr).GlobalIrq,
                        polarity,
                        trigger_mode,
                  });
            }),
      ];
      super::parse_madt(madt, parser);

      Ok(())
}

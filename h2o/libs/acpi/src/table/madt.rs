use crate::raw;
use crate::subt_parser;
use paging::PAddr;

use alloc::{vec, vec::Vec};
use core::mem::size_of;
use core::ptr::null_mut;

const INIT_BASE_ADDR: PAddr = PAddr::new(0xFEE00000);

#[derive(Copy, Clone, Debug)]
pub struct LapicNode {
      pub id: u32,
      pub acpi_id: u32,
}

#[derive(Copy, Clone, Debug)]
pub enum LapicType {
      X2,
      X1(PAddr),
}

#[derive(Clone, Debug)]
pub struct LapicData {
      pub ty: LapicType,
      pub lapics: Vec<LapicNode>,
}

#[inline]
unsafe fn parse_madt(madt: *mut raw::ACPI_TABLE_MADT, parser: Vec<super::SubtableParser>) {
      super::parse_subtable(madt.cast(), size_of::<raw::ACPI_TABLE_MADT>(), parser)
}

static mut IS_X2: bool = false;
static mut BASE_ADDR: PAddr = INIT_BASE_ADDR;
static mut LAPICS: Vec<LapicNode> = Vec::new();
/// # Safety
///
/// The caller must ensure that the memory mapping for ACPI tables is fixed and valid,
/// and that this function is not called twice or after SMP initialization.
pub unsafe fn get_lapic_data() -> Result<LapicData, raw::ACPI_STATUS> {
      let madt = {
            let mut tbl = null_mut();
            let status = raw::AcpiGetTable(raw::ACPI_SIG_MADT.as_ptr() as _, 0, &mut tbl);
            if status != raw::AE_OK {
                  return Err(status);
            }
            tbl.cast()
      };

      IS_X2 = raw_cpuid::CpuId::new()
            .get_feature_info()
            .map_or(false, |info| info.has_x2apic());
      BASE_ADDR = INIT_BASE_ADDR;
      LAPICS = Vec::new();

      let parser = vec![
            subt_parser!(raw::ACPI_MADT_TYPE_LOCAL_APIC_OVERRIDE => box |subt| {
                  let lapic_ovr = subt.cast::<raw::ACPI_MADT_LOCAL_APIC_OVERRIDE>();
                  BASE_ADDR = PAddr::new((*lapic_ovr).Address as usize);
            }),
            subt_parser!(raw::ACPI_MADT_TYPE_LOCAL_X2APIC => box |subt| {
                  let x2apic = subt.cast::<raw::ACPI_MADT_LOCAL_X2APIC>();
                  if (*x2apic).LocalApicId != 0u32.wrapping_sub(1)
                        && ((*x2apic).LapicFlags & raw::ACPI_MADT_ENABLED) != 0
                  {
                        IS_X2 = true;
                        LAPICS.push(LapicNode {
                              id: (*x2apic).LocalApicId,
                              acpi_id: (*x2apic).Uid,
                        });
                  }
            }),
            subt_parser!(raw::ACPI_MADT_TYPE_LOCAL_APIC => box |subt| {
                  let apic = subt.cast::<raw::ACPI_MADT_LOCAL_APIC>();
                  if (*apic).Id != 0u8.wrapping_sub(1)
                        && ((*apic).LapicFlags & raw::ACPI_MADT_ENABLED) != 0
                  {
                        LAPICS.push(LapicNode {
                              id: (*apic).Id as u32,
                              acpi_id: (*apic).ProcessorId as u32,
                        });
                  }
            }),
      ];
      parse_madt(madt, parser);

      let lapic_data = LapicData {
            ty: if IS_X2 {
                  LapicType::X2
            } else {
                  LapicType::X1(BASE_ADDR)
            },
            lapics: LAPICS.clone(),
      };
      Ok(lapic_data)
}

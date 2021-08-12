use crate::raw;
use paging::PAddr;

use core::ptr::null_mut;
use core::usize;

pub struct HpetData {
      pub base: PAddr,
      pub block_id: u8,
}

/// # Safety
///
/// The caller must ensure that the memory mapping for ACPI tables is fixed and valid,
/// and that this function is not called twice or after SMP initialization.
pub unsafe fn get_hpet_data() -> Result<HpetData, raw::ACPI_STATUS> {
      let hpet = {
            let mut tbl = null_mut();
            let status = raw::AcpiGetTable(raw::ACPI_SIG_HPET.as_ptr() as _, 0, &mut tbl);
            if status != raw::AE_OK {
                  return Err(status);
            }
            tbl.cast::<raw::ACPI_TABLE_HPET>()
      };
      if (*hpet).Address.SpaceId != 0 {
            return Err(raw::AE_ERROR);
      }

      let addr_val = {
            let val = (*hpet).Address.Address;
            if val == 0xFED0000000000000 {
                  val >> 32
            } else {
                  val
            }
      };

      Ok(HpetData {
            base: PAddr::new(addr_val as usize),
            block_id: (*hpet).Sequence,
      })
}

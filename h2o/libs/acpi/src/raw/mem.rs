use super::*;
use paging::{LAddr, PAddr};

use alloc::collections::BTreeMap;
use core::alloc::Layout;
use core::ops::Range;
use core::ptr::null_mut;
use spin::Mutex;

const PHYS_RANGE: Range<PAddr> = PAddr::new(0)..PAddr::new(minfo::INITIAL_ID_SPACE);
const VIRT_RANGE: Range<LAddr> = LAddr::new(minfo::ID_OFFSET as *mut u8)
      ..LAddr::new((minfo::ID_OFFSET + minfo::INITIAL_ID_SPACE) as *mut u8);

static ALLOC_REC: Mutex<BTreeMap<usize, Layout>> = Mutex::new(BTreeMap::new());

#[no_mangle]
unsafe extern "C" fn AcpiOsMapMemory(paddr: ACPI_PHYSICAL_ADDRESS, len: ACPI_SIZE) -> *mut u8 {
      let paddr = PAddr::new(paddr as usize);
      assert!(PHYS_RANGE.contains(&paddr));

      *paddr.to_laddr(minfo::ID_OFFSET)
}

#[no_mangle]
unsafe extern "C" fn AcpiOsUnmapMemory(_laddr: *mut u8, _len: ACPI_SIZE) {}

#[no_mangle]
unsafe extern "C" fn AcpiOsGetPhysicalAddress(
      laddr: *mut u8,
      paddr: *mut ACPI_PHYSICAL_ADDRESS,
) -> ACPI_STATUS {
      let laddr = LAddr::new(laddr);

      if laddr.is_null() || !paddr.is_null() {
            AE_BAD_PARAMETER
      } else if VIRT_RANGE.contains(&laddr) {
            *paddr = *laddr.to_paddr(minfo::ID_OFFSET) as ACPI_PHYSICAL_ADDRESS;
            AE_OK
      } else {
            AE_ERROR
      }
}

#[no_mangle]
unsafe extern "C" fn AcpiOsReadMemory(
      paddr: ACPI_PHYSICAL_ADDRESS,
      val: *mut UINT64,
      width: UINT32,
) -> ACPI_STATUS {
      unimplemented!()
}

#[no_mangle]
unsafe extern "C" fn AcpiOsWriteMemory(
      paddr: ACPI_PHYSICAL_ADDRESS,
      val: UINT64,
      width: UINT32,
) -> ACPI_STATUS {
      unimplemented!()
}

#[no_mangle]
unsafe extern "C" fn AcpiOsAllocate(size: ACPI_SIZE) -> *mut u8 {
      let size = size as usize;
      Layout::from_size_align(size, size.next_power_of_two()).map_or(null_mut(), |layout| {
            let ptr = alloc::alloc::alloc(layout);
            if !ptr.is_null() {
                  let mut rec = ALLOC_REC.lock();
                  rec.insert(ptr as usize, layout);
            }
            ptr
      })
}

#[no_mangle]
unsafe extern "C" fn AcpiOsFree(ptr: *mut u8) {
      let layout = {
            let mut rec = ALLOC_REC.lock();
            rec.remove(&(ptr as usize))
      };

      if let Some(layout) = layout {
            alloc::alloc::dealloc(ptr, layout);
      }
}

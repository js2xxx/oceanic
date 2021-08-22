#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(unused)]
#![allow(clippy::redundant_static_lifetimes)]

mod mem;
mod outp;
mod table;

use cty::*;

include!(concat!(env!("OUT_DIR"), "/acpica.rs"));

pub const AE_OK: ACPI_STATUS = 0;

pub const AE_ERROR: ACPI_STATUS = AE_CODE_ENVIRONMENTAL | 1;
pub const AE_NO_ACPI_TABLES: ACPI_STATUS = AE_CODE_ENVIRONMENTAL | 2;
pub const AE_NO_MEMORY: ACPI_STATUS = AE_CODE_ENVIRONMENTAL | 4;
pub const AE_NOT_FOUND: ACPI_STATUS = AE_CODE_ENVIRONMENTAL | 5;
pub const AE_NOT_IMPLEMENTED: ACPI_STATUS = AE_CODE_ENVIRONMENTAL | 0xe;
pub const AE_LIMIT: ACPI_STATUS = AE_CODE_ENVIRONMENTAL | 0x10;
pub const AE_TIME: ACPI_STATUS = AE_CODE_ENVIRONMENTAL | 0x11;
pub const AE_BAD_PARAMETER: ACPI_STATUS = AE_CODE_PROGRAMMER | 1;

pub(super) static mut RSDP: usize = 0;

#[no_mangle]
unsafe extern "C" fn AcpiOsGetRootPointer() -> ACPI_PHYSICAL_ADDRESS {
      RSDP as ACPI_PHYSICAL_ADDRESS
}

#[no_mangle]
extern "C" fn AcpiOsInitialize() -> ACPI_STATUS {
      AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsTerminate() -> ACPI_STATUS {
      AE_OK
}

#[no_mangle]
unsafe extern "C" fn AcpiOsGetTimer() -> UINT64 {
      crate::cpu::time::Instant::now().raw() as u64
}

#[no_mangle]
unsafe extern "C" fn AcpiOsCreateLock(OutHandle: *mut *mut u8) -> ACPI_STATUS {
      unimplemented!()
}

#[no_mangle]
unsafe extern "C" fn AcpiOsDeleteLock(Handle: *mut u8) {
      unimplemented!()
}

#[no_mangle]
unsafe extern "C" fn AcpiOsAcquireLock(Handle: *mut u8) -> u64 {
      unimplemented!()
}

#[no_mangle]
unsafe extern "C" fn AcpiOsReleaseLock(Handle: *mut u8, Flags: u64) {
      unimplemented!()
}

#[no_mangle]
unsafe extern "C" fn AcpiOsCreateSemaphore(
      MaxUnits: UINT32,
      InitialUnits: UINT32,
      OutHandle: *mut *mut cty::c_void,
) -> ACPI_STATUS {
      unimplemented!()
}

#[no_mangle]
unsafe extern "C" fn AcpiOsDeleteSemaphore(Handle: *mut cty::c_void) -> ACPI_STATUS {
      unimplemented!()
}

#[no_mangle]
unsafe extern "C" fn AcpiOsWaitSemaphore(
      Handle: *mut cty::c_void,
      Units: UINT32,
      Timeout: UINT16,
) -> ACPI_STATUS {
      if AcpiSubsystemStatus() != AE_OK {
            AE_OK
      } else {
            unimplemented!()
      }
}

#[no_mangle]
unsafe extern "C" fn AcpiOsSignalSemaphore(Handle: *mut cty::c_void, Units: UINT32) -> ACPI_STATUS {
      if AcpiSubsystemStatus() != AE_OK {
            AE_OK
      } else {
            unimplemented!()
      }
}

#[no_mangle]
unsafe extern "C" fn AcpiOsInstallInterruptHandler(
      InterruptNumber: UINT32,
      ServiceRoutine: ACPI_OSD_HANDLER,
      Context: *mut cty::c_void,
) -> ACPI_STATUS {
      unimplemented!();
}
#[no_mangle]
unsafe extern "C" fn AcpiOsRemoveInterruptHandler(
      InterruptNumber: UINT32,
      ServiceRoutine: ACPI_OSD_HANDLER,
) -> ACPI_STATUS {
      unimplemented!();
}

#[no_mangle]
unsafe extern "C" fn AcpiOsGetThreadId() -> UINT64 {
      0
}
#[no_mangle]
unsafe extern "C" fn AcpiOsExecute(
      Type: ACPI_EXECUTE_TYPE,
      Function: ACPI_OSD_EXEC_CALLBACK,
      Context: *mut cty::c_void,
) -> ACPI_STATUS {
      unimplemented!();
}
#[no_mangle]
unsafe extern "C" fn AcpiOsWaitEventsComplete() {
      unimplemented!();
}
#[no_mangle]
unsafe extern "C" fn AcpiOsSleep(Milliseconds: UINT64) {
      unimplemented!();
}
#[no_mangle]
unsafe extern "C" fn AcpiOsStall(Microseconds: UINT32) {
      unimplemented!();
}

#[no_mangle]
unsafe extern "C" fn AcpiOsReadPort(
      Address: ACPI_IO_ADDRESS,
      Value: *mut UINT32,
      Width: UINT32,
) -> ACPI_STATUS {
      unimplemented!();
}

#[no_mangle]
unsafe extern "C" fn AcpiOsWritePort(
      Address: ACPI_IO_ADDRESS,
      Value: UINT32,
      Width: UINT32,
) -> ACPI_STATUS {
      unimplemented!();
}

#[no_mangle]
unsafe extern "C" fn AcpiOsReadPciConfiguration(
      PciId: *mut ACPI_PCI_ID,
      Reg: UINT32,
      Value: *mut UINT64,
      Width: UINT32,
) -> ACPI_STATUS {
      unimplemented!();
}
#[no_mangle]
unsafe extern "C" fn AcpiOsWritePciConfiguration(
      PciId: *mut ACPI_PCI_ID,
      Reg: UINT32,
      Value: UINT64,
      Width: UINT32,
) -> ACPI_STATUS {
      unimplemented!();
}
#[no_mangle]
unsafe extern "C" fn AcpiOsSignal(Function: UINT32, Info: *mut cty::c_void) -> ACPI_STATUS {
      unimplemented!()
}

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(unused)]
#![allow(clippy::redundant_static_lifetimes)]

mod mem;

use ::va_list::VaList;
use alloc::borrow::ToOwned;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use cty::*;
use spin::Mutex;

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

static PRINT_BUF: Mutex<String> = Mutex::new(String::new());

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
      unimplemented!()
}

#[no_mangle]
unsafe extern "C" fn AcpiOsPredefinedOverride(
      obj: *const ACPI_PREDEFINED_NAMES,
      newv: ACPI_STRING,
) -> ACPI_STATUS {
      *newv = '\0' as i8;
      AE_OK
}

#[no_mangle]
unsafe extern "C" fn AcpiOsTableOverride(
      ExistingTable: *const ACPI_TABLE_HEADER,
      NewTable: *mut (*const ACPI_TABLE_HEADER),
) -> ACPI_STATUS {
      *NewTable = core::ptr::null_mut();
      AE_OK
}

#[no_mangle]
unsafe extern "C" fn AcpiOsPhysicalTableOverride(
      ExistingTable: *const ACPI_TABLE_HEADER,
      NewAddress: *mut ACPI_PHYSICAL_ADDRESS,
      NewTableLength: *mut UINT32,
) -> ACPI_STATUS {
      *NewAddress = 0;
      *NewTableLength = 0;
      AE_OK
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

extern "C" {
      #[no_mangle]
      fn strlen(s: *const u8) -> usize;

      #[no_mangle]
      fn vsnprintf(buf: *const u8, len: usize, fmt: *const u8, args: VaList) -> cty::c_int;
}

#[no_mangle]
unsafe extern "C" fn AcpiOsVprintf(format: *const u8, args: VaList) {
      let mut buf = PRINT_BUF.lock();

      let mut new_buf = {
            let len = strlen(format);
            let slice = core::slice::from_raw_parts(format, len as usize);
            let mut buf = Vec::with_capacity(256);
            buf.extend_from_slice(slice);
            {
                  let ptr = buf.as_mut_ptr();
                  vsnprintf(ptr, 256, format, args);
            }
            buf
      };

      let mut input: &[u8] = &new_buf;
      loop {
            match core::str::from_utf8(input) {
                  Ok(valid) => {
                        buf.push_str(valid);
                        break;
                  }
                  Err(error) => {
                        let (valid, after_valid) = input.split_at(error.valid_up_to());
                        unsafe { buf.push_str(core::str::from_utf8_unchecked(valid)) }
                        buf.push('\u{FFFD}');

                        if let Some(invalid_sequence_length) = error.error_len() {
                              input = &after_valid[invalid_sequence_length..]
                        } else {
                              break;
                        }
                  }
            }
      }

      if buf.ends_with('\n') {
            buf.pop();
            log::info!("{}", &buf);
            buf.clear();
      }
}

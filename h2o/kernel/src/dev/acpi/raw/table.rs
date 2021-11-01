use super::*;

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

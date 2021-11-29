use paging::PAddr;

#[derive(Debug, Clone)]
pub struct Handler;

impl acpi::AcpiHandler for Handler {
    unsafe fn map_physical_region<T>(
        &self,
        phys: usize,
        size: usize,
    ) -> acpi::PhysicalMapping<Self, T> {
        let virt = PAddr::new(phys)
            .to_laddr(minfo::ID_OFFSET)
            .as_non_null()
            .unwrap()
            .cast::<T>();
        acpi::PhysicalMapping::new(phys, virt, size, size, Self)
    }

    fn unmap_physical_region<T>(_: &acpi::PhysicalMapping<Self, T>) {}
}

static mut TABLES: Option<acpi::AcpiTables<Handler>> = None;
static mut PLATFORM_INFO: Option<acpi::PlatformInfo> = None;

/// # Safety
///
/// TODO: Remove `pub` or `unsafe` after module implementation.
pub unsafe fn init_tables(rsdp: usize) {
    let tables = acpi::AcpiTables::from_rsdp(Handler, rsdp).expect("Failed to get ACPI tables");
    debug_assert!(TABLES.is_none());
    PLATFORM_INFO = Some(tables.platform_info().expect("Failed to get platform info"));
    TABLES = Some(tables);
}

pub unsafe fn tables() -> &'static acpi::AcpiTables<Handler> {
    TABLES.as_ref().unwrap()
}

pub unsafe fn platform_info() -> &'static acpi::PlatformInfo {
    PLATFORM_INFO.as_ref().unwrap()
}

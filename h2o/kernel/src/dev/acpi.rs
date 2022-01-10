use paging::PAddr;
use spin::Lazy;

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

static TABLES: Lazy<acpi::AcpiTables<Handler>> = Lazy::new(|| unsafe {
    acpi::AcpiTables::from_rsdp(Handler, *crate::kargs().rsdp).expect("Failed to get ACPI tables")
});
static PLATFORM_INFO: Lazy<acpi::PlatformInfo> =
    Lazy::new(|| TABLES.platform_info().expect("Failed to get platform info"));

#[inline]
pub fn tables() -> &'static acpi::AcpiTables<Handler> {
    &*TABLES
}

#[inline]
pub fn platform_info() -> &'static acpi::PlatformInfo {
    &*PLATFORM_INFO
}

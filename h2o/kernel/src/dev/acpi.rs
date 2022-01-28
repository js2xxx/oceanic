use archop::Azy;
use paging::PAddr;

#[derive(Debug, Clone)]
pub struct Handler;

impl acpi::AcpiHandler for Handler {
    unsafe fn map_physical_region<T>(
        &self,
        phys: usize,
        size: usize,
    ) -> acpi::PhysicalMapping<Self, T> {
        let virt = unsafe {
            PAddr::new(phys)
                .to_laddr(minfo::ID_OFFSET)
                .as_non_null_unchecked()
        }
        .cast::<T>();
        acpi::PhysicalMapping::new(phys, virt, size, size, Self)
    }

    fn unmap_physical_region<T>(_: &acpi::PhysicalMapping<Self, T>) {}
}

static TABLES: Azy<acpi::AcpiTables<Handler>> = Azy::new(|| unsafe {
    acpi::AcpiTables::from_rsdp(Handler, *crate::kargs().rsdp).expect("Failed to get ACPI tables")
});
static PLATFORM_INFO: Azy<acpi::PlatformInfo> =
    Azy::new(|| TABLES.platform_info().expect("Failed to get platform info"));

#[inline]
pub fn tables() -> &'static acpi::AcpiTables<Handler> {
    &*TABLES
}

#[inline]
pub fn platform_info() -> &'static acpi::PlatformInfo {
    &*PLATFORM_INFO
}

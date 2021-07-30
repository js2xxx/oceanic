pub mod ioapic;
pub mod lpic;
pub mod hpet;

/// Initialize interrupt chips.
///
/// # Safety
///
/// This function must be called only once from the bootstrap CPU.
pub unsafe fn init_intr_chip(ioapic_data: acpi::table::madt::IoapicData) {
      lpic::init(true);
      ioapic::init(ioapic_data);
}
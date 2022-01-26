pub mod hpet;
pub mod ioapic;
pub mod lpic;

/// Initialize interrupt chips.
///
/// # Panics
///
/// Panics if the hardware don't support APIC interrupt model.
///
/// # Safety
///
/// This function must be called only once from the bootstrap CPU.
pub unsafe fn init_intr_chip() {
    let ioapic_data = match crate::dev::acpi::platform_info().interrupt_model {
        acpi::InterruptModel::Apic(ref apic) => apic,
        _ => panic!("Failed to get IOAPIC data"),
    };
    if ioapic_data.also_has_legacy_pics {
        lpic::init(true);
    }
    let _ = ioapic::chip();
}

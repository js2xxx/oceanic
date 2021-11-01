pub mod ioapic;
pub mod lapic;

use alloc::vec::Vec;
use core::mem::size_of;

pub use ioapic::{get_ioapic_data, IntrOvr, IoapicData, IoapicNode};
pub use lapic::{get_lapic_data, LapicData, LapicNode, LapicType};

use super::raw;
use crate::dev::acpi::table;

#[inline]
unsafe fn parse_madt(madt: *mut raw::ACPI_TABLE_MADT, parser: Vec<table::SubtableParser>) {
    table::parse_subtable(madt.cast(), size_of::<raw::ACPI_TABLE_MADT>(), parser)
}

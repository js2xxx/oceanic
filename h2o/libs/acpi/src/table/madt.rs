pub mod ioapic;
pub mod lapic;

pub use lapic::{get_lapic_data, LapicData, LapicNode, LapicType};
pub use ioapic::{get_ioapic_data, IoapicData, IoapicNode, IntrOvr};

use crate::raw;

use alloc::vec::Vec;
use core::mem::size_of;

#[inline]
unsafe fn parse_madt(madt: *mut raw::ACPI_TABLE_MADT, parser: Vec<crate::table::SubtableParser>) {
      crate::table::parse_subtable(madt.cast(), size_of::<raw::ACPI_TABLE_MADT>(), parser)
}

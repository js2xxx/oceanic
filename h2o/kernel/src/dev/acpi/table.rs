pub mod hpet;
pub mod madt;

use alloc::{boxed::Box, vec::Vec};

pub use madt::{get_ioapic_data, get_lapic_data};

use super::raw;

struct SubtableParser {
    ty_idx: u32,
    handler: Box<dyn FnMut(*mut raw::ACPI_SUBTABLE_HEADER)>,
}

unsafe fn parse_subtable(
    table: *mut raw::ACPI_TABLE_HEADER,
    header_size: usize,
    mut parser: Vec<SubtableParser>,
) {
    let len = (*table).Length as usize;

    let mut ptr = table.cast::<u8>().add(header_size);
    while ptr < table.cast::<u8>().add(len) {
        let subt = ptr.cast::<raw::ACPI_SUBTABLE_HEADER>();

        for p in parser.iter_mut() {
            if (*subt).Type as u32 == p.ty_idx {
                (*p.handler)(subt);
            }
        }

        let subt_len = (*subt).Length as usize;
        ptr = ptr.add(subt_len);
    }
}

#[macro_export]
macro_rules! subt_parser {
    ($ty_idx:expr => $handler:expr) => {
        $crate::dev::acpi::table::SubtableParser {
            ty_idx: $ty_idx,
            handler: $handler,
        }
    };
}

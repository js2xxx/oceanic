use core::{alloc::Layout, mem};

use static_assertions::const_assert;

#[cfg(target_arch = "x86_64")]
pub const PAGE_LAYOUT: Layout = unsafe { Layout::from_size_align_unchecked(4096, 4096) };

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum EntryType {
    File,
    /// The content of the directory only contains an array of offsets of other
    /// entries, pointing to the global entry table.
    Directory,
}

#[derive(Debug, Copy, Clone)]
#[repr(C, align(128))]
pub struct Entry {
    pub version: u32,
    pub name: [u8; 64],
    pub ty: EntryType,
    pub offset: usize,
    pub len: usize,
}
const_assert!(mem::size_of::<Entry>() <= 128);
pub const ENTRY_LAYOUT: Layout = Layout::new::<Entry>();

pub const MAX_NAME_LEN: usize = 64;

/// The header of the bootfs.
#[derive(Debug, Copy, Clone)]
#[repr(C, align(128))]
pub struct BootfsHeader {
    pub version: u32,
    pub num_entries: usize,
    pub root_dir_offset: usize,
    pub root_dir_len: usize,
}

pub const VERSION: u32 = u32::from_ne_bytes([0xbb, 0xff, 0xee, 0xaa]);

pub const HEADER_SIZE: usize =
    mem::size_of::<BootfsHeader>().next_multiple_of(mem::size_of::<usize>());

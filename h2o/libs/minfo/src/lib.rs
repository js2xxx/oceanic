#![no_std]

use core::alloc::Layout;

// Physical addresses

pub const KARGS_BASE: usize = 0;

pub const TRAMPOLINE_RANGE: core::ops::Range<usize> = 0..0x100000;

pub const INITIAL_ID_SPACE: usize = 0x1_0000_0000;

pub use pmm::{KMEM_PHYS_BASE, PF_SIZE};

// Virtual addresses

pub const USER_BASE: usize = 0x100000;

pub const USER_TLS_BASE: usize = 0x7F00_0000_0000;
pub const USER_TLS_END: usize = USER_STACK_BASE;

pub const USER_STACK_BASE: usize = 0x7F80_0000_0000;

pub const USER_END: usize = 0x7FFF_0000_0000;

pub const KERNEL_SPACE_START: usize = 0xFFFF_8000_0000_0000;

pub const KERNEL_ALLOCABLE_RANGE: core::ops::Range<pmm::LAddr> =
    pmm::LAddr::new(0xFFFF_A000_0000_0000 as *mut u8)
        ..pmm::LAddr::new(0xFFFF_F000_0000_0000 as *mut u8);

pub const ID_OFFSET: usize = KERNEL_SPACE_START;

// Kernel args

#[derive(Debug, Copy, Clone)]
pub struct KernelArgs {
    pub rsdp: paging::PAddr,

    pub efi_mmap_paddr: paging::PAddr,
    pub efi_mmap_len: usize,
    pub efi_mmap_unit: usize,

    pub pls_layout: Option<Layout>,

    pub tinit_phys: paging::PAddr,
    pub tinit_len: usize,
}

#![no_std]
// Physical addresses

pub const KARGS_BASE: usize = 0;

pub const TRAMPOLINE_RANGE: core::ops::Range<usize> = 0..0x100000;

pub const INITIAL_ID_SPACE: usize = 0x1_0000_0000;

pub use pmm::KMEM_PHYS_BASE;

pub use pmm::PF_SIZE;

// Virtual addresses

pub const USER_BASE: usize = 0x100000;

pub const USER_END: usize = 0x7FFF_0000_0000;

pub const KERNEL_SPACE_START: usize = 0xFFFF_8000_0000_0000;

pub const KERNEL_ALLOCABLE_RANGE: core::ops::Range<pmm::LAddr> =
      pmm::LAddr::new(0xFFFF_A000_0000_0000 as *mut u8)
            ..pmm::LAddr::new(0xFFFF_F000_0000_0000 as *mut u8);

pub const ID_OFFSET: usize = KERNEL_SPACE_START;

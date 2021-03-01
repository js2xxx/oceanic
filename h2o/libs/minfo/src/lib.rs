#![no_std]

pub const USER_BASE: usize = 0x100000;

pub const INITIAL_ID_SPACE: usize = 0x1_0000_0000;

pub const USER_END: usize = 0x7FFF_0000_0000;

pub const KERNEL_SPACE_START: usize = 0xFFFF_8000_0000_0000;

pub use pmm::KMEM_PHYS_BASE;

pub const ID_OFFSET: usize = KERNEL_SPACE_START;

pub use pmm::PF_SIZE;
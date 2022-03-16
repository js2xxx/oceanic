#![no_std]

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(usize)]
pub enum HandleIndex {
    MemRes = 0,
    PioRes = 1,
    GsiRes = 2,
    Vdso = 3,
    Bootfs = 4,
}

#[derive(Debug, Copy, Clone)]
pub struct Targs {
    pub rsdp: usize,
    pub smbios: usize,
    pub bootfs_size: usize,
}

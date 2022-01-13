#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Exception {
    pub vec: u8,
    pub errc: u64,
    pub cr2: u64,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ExceptionResult {
    pub code: u64,
}

pub const EXRES_CODE_OK: u64 = 1;

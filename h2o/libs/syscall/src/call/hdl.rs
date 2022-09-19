use crate::SerdeReg;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Handle {
    raw: u32,
}

impl Handle {
    pub const NULL: Handle = Handle { raw: 0 };

    pub fn new(raw: u32) -> Handle {
        Handle { raw }
    }

    pub fn check_null(&self) -> Result<Self, crate::Error> {
        if self.raw != 0 {
            Ok(*self)
        } else {
            Err(crate::EINVAL)
        }
    }

    pub fn is_null(&self) -> bool {
        self.raw == 0
    }

    pub fn raw(&self) -> u32 {
        self.raw
    }
}

impl TryFrom<*mut u8> for Handle {
    type Error = crate::Error;

    fn try_from(value: *mut u8) -> Result<Self, Self::Error> {
        let raw = value as u32;
        Handle { raw }.check_null()
    }
}

impl SerdeReg for Handle {
    fn encode(self) -> usize {
        self.raw as usize
    }

    fn decode(val: usize) -> Self {
        Handle { raw: val as u32 }
    }
}

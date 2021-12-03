use crate::SerdeReg;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Handle(u32);

impl Handle {
    pub fn new(raw: u32) -> Handle {
        Handle(raw)
    }

    pub fn check_null(&self) -> Result<Self, crate::Error> {
        if self.0 != 0 {
            Ok(Handle(self.0))
        } else {
            Err(crate::Error(crate::EINVAL))
        }
    }

    pub fn raw(&self) -> u32 {
        self.0
    }
}

impl TryFrom<*mut u8> for Handle {
    type Error = crate::Error;

    fn try_from(value: *mut u8) -> Result<Self, Self::Error> {
        let raw = value as u32;
        Handle(raw).check_null()
    }
}

impl SerdeReg for Handle {
    fn encode(self) -> usize {
        self.0 as usize
    }

    fn decode(val: usize) -> Self {
        Handle(val as u32)
    }
}

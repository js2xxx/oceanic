use crate::{Error, Handle, Result, SerdeReg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct Status(Error);

impl SerdeReg for Status {
    fn encode(self) -> usize {
        (-self.0.raw()) as usize
    }

    fn decode(val: usize) -> Self {
        Self(Error::try_decode(val).unwrap_or(Error::OK))
    }
}

impl Status {
    #[inline]
    pub fn into_res(self) -> Result {
        if self.0 == Error::OK {
            Ok(())
        } else {
            Err(self.0)
        }
    }

    #[inline]
    pub fn from_res(res: Result) -> Self {
        SerdeReg::decode(res.encode())
    }
}

impl Default for Status {
    fn default() -> Self {
        Status(Error::OK)
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
pub union StatusOrHandle {
    pub handle: Handle,
    pub error: Error,
    _value: usize,
}

impl SerdeReg for StatusOrHandle {
    fn encode(self) -> usize {
        unsafe { self._value }
    }

    fn decode(value: usize) -> Self {
        StatusOrHandle { _value: value }
    }
}

impl StatusOrHandle {
    #[inline]
    pub fn into_res(self) -> Result<Handle> {
        SerdeReg::decode(self.encode())
    }

    #[inline]
    pub fn from_res(res: Result<Handle>) -> Self {
        SerdeReg::decode(res.encode())
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
pub union StatusOrValue {
    pub value: u64,
    pub error: Error,
}

impl SerdeReg for StatusOrValue {
    fn encode(self) -> usize {
        unsafe { self.value as usize }
    }

    fn decode(val: usize) -> Self {
        Self { value: val as u64 }
    }
}

impl StatusOrValue {
    #[inline]
    pub fn into_res(self) -> Result<u64> {
        SerdeReg::decode(self.encode())
    }

    #[inline]
    pub fn from_res(res: Result<u64>) -> Self {
        SerdeReg::decode(res.encode())
    }
}

impl Default for StatusOrValue {
    fn default() -> Self {
        StatusOrValue { value: 0 }
    }
}

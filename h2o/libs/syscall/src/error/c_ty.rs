use crate::{Error, Handle, SerdeReg};

#[repr(transparent)]
pub struct Status(Error);

impl SerdeReg for Status {
    fn encode(self) -> usize {
        -self.0.raw() as usize
    }

    fn decode(val: usize) -> Self {
        Self(Error::try_decode(val).unwrap_or(Error::OK))
    }
}

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

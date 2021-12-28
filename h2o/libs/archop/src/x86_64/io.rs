//! I/O operations for the kernel

use core::{arch::asm, marker::PhantomData};

use crate::io::Io;

pub struct Port<T> {
    port: u16,
    _marker: PhantomData<T>,
}

impl<T> Port<T> {
    /// Creates a new x86_64 port.
    ///
    /// # Safety
    ///
    /// The `port` must be valid; more precisely, the port must be present and
    /// available.
    pub const unsafe fn new(port: u16) -> Port<T> {
        Port {
            port,
            _marker: PhantomData,
        }
    }
}

impl Io for Port<u8> {
    type Val = u8;
    type Off = u16;

    unsafe fn read_offset(&self, offset: u16) -> Self::Val {
        let ret;
        asm!("in al, dx", out("al") ret, in("dx") (self.port + offset));
        ret
    }

    unsafe fn write_offset(&mut self, offset: u16, value: Self::Val) {
        asm!("out dx, al", in("dx") (self.port + offset), in("al") value);
    }
}

impl Io for Port<u16> {
    type Val = u16;
    type Off = u16;

    unsafe fn read_offset(&self, offset: u16) -> Self::Val {
        let ret;
        asm!("in ax, dx", out("ax") ret, in("dx") (self.port + offset));
        ret
    }

    unsafe fn write_offset(&mut self, offset: u16, value: Self::Val) {
        asm!("out dx, ax", in("dx") (self.port + offset), in("ax") value);
    }
}

impl Io for Port<u32> {
    type Val = u32;
    type Off = u16;

    unsafe fn read_offset(&self, offset: u16) -> Self::Val {
        let ret;
        asm!("in eax, dx", out("eax") ret, in("dx") (self.port + offset));
        ret
    }

    unsafe fn write_offset(&mut self, offset: u16, value: Self::Val) {
        asm!("out dx, eax", in("dx") (self.port + offset), in("eax") value);
    }
}

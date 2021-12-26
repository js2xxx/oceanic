use core::{fmt, hint};

use archop::io::{Io, Port};

/// The COM port for logging.
pub(super) const COM_LOG: u16 = 0x3f8;

/// The output struct interface.
pub struct Output(Port<u8>);

impl Output {
    /// Initialize the serial port. Copied from Osdev Wiki.
    pub unsafe fn new(port: u16) -> Output {
        // SAFE: The port is present and available.
        let mut sp = unsafe { Port::new(port) };
        // SAFE: These offsets and values are valid.
        unsafe {
            sp.write_offset(1, 0x00); // Disable all interrupts
            sp.write_offset(3, 0x80); // Enable DLAB (set baud rate divisor)
            sp.write_offset(0, 0x03); // Set divisor to 3 (lo byte) 38400 baud
            sp.write_offset(1, 0x00); //                  (hi byte)
            sp.write_offset(3, 0x03); // 8 bits, no parity, one stop bit
            sp.write_offset(2, 0xC7); // Enable FIFO, clear them, with 14-byte threshold
            sp.write_offset(4, 0x0B); // IRQs enabled, RTS/DSR set
        }
        Output(sp)
    }
}

impl Output {
    // unsafe fn has_data(&self) -> bool {
    //       (self.0.read_offset(5) & 1) != 0
    // }

    unsafe fn buf_full(&self) -> bool {
        (self.0.read_offset(5) & 0x20) == 0
    }

    // unsafe fn in_char(&self) -> u8 {
    //       while has_data() {
    //             core::hint::spin_loop();
    //       }
    //       self.0.read()
    // }

    /// Output a character byte to the serial port for logging.
    unsafe fn out_char(&mut self, c: u8) {
        while self.buf_full() {
            hint::spin_loop();
        }
        self.0.write(c);
    }
}

impl fmt::Write for Output {
    fn write_str(&mut self, s: &str) -> Result<(), fmt::Error> {
        for b in s.bytes() {
            unsafe { self.out_char(b) };
        }
        Ok(())
    }
}

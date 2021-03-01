use archop::io;

use core::{fmt, hint};

/// The COM port for logging.
const COM_LOG: u16 = 0x3f8;

/// The output struct interface.
pub struct Output;

impl Output {
      /// Initialize the serial port. Copied from Osdev Wiki.
      pub fn new() -> Output {
            unsafe {
                  io::out8(COM_LOG + 1, 0x00); // Disable all interrupts
                  io::out8(COM_LOG + 3, 0x80); // Enable DLAB (set baud rate divisor)
                  io::out8(COM_LOG, 0x03); // Set divisor to 3 (lo byte) 38400 baud
                  io::out8(COM_LOG + 1, 0x00); //              (hi byte)
                  io::out8(COM_LOG + 3, 0x03); // 8 bits, no parity, one stop bit
                  io::out8(COM_LOG + 2, 0xC7); // Enable FIFO, clear them, with 14-byte threshold
                  io::out8(COM_LOG + 4, 0x0B); // IRQs enabled, RTS/DSR set
            }
            Output
      }
}

impl fmt::Write for Output {
      #[inline]
      fn write_str(&mut self, s: &str) -> Result<(), fmt::Error> {
            for b in s.bytes() {
                  unsafe { out_char(b) };
            }
            Ok(())
      }
}

// unsafe fn has_data() -> bool {
//       (io::in8(COM_LOG + 5) & 1) != 0
// }

unsafe fn buf_full() -> bool {
      (io::in8(COM_LOG + 5) & 0x20) == 0
}

// unsafe fn in_char() -> u8 {
//       while has_data() {
//             core::hint::spin_loop();
//       }
//       io::in8(COM_LOG)
// }

/// Output a char to the serial port for logging.
#[inline]
unsafe fn out_char(c: u8) {
      while buf_full() {
            hint::spin_loop();
      }
      io::out8(COM_LOG, c);
}

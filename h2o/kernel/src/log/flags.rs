//! Flags output
//!
//! In the kernel's development, we need to check a series of flags in integers sometimes.
//! A nice output can simplifies the debugging process.
//!
//! See [`Flags`] for more.

use core::fmt::{Display, Error, Formatter};
use core::str;

/// A series of flags for nice output.
///
/// We use cases of letters to indicate every bit flags' value.
pub struct Flags {
      /// The value of the flags.
      value: u64,
      /// The names of the bits of the flags. Ascending from low bits to great.
      ///
      /// # Examples
      ///
      ///                0b011
      ///     "A B C" ->   CBA --output--> "A B c"
      format: &'static str,
}

/// A simple word buffer for output.
static mut STR_BUF: [u8; 256] = [0; 256];

impl Display for Flags {
      fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
            unsafe {
                  for (i, word) in self.format.split_whitespace().enumerate() {
                        let k = &mut STR_BUF[0..word.len()];
                        k.copy_from_slice(word.as_bytes());
                        let k = str::from_utf8_mut(k).unwrap();

                        let b = (self.value >> i) & 1;
                        if b != 0 {
                              k.make_ascii_uppercase();
                        } else {
                              k.make_ascii_lowercase();
                        }
                        write!(f, "{} ", k)?;
                  }
                  Ok(())
            }
      }
}

impl Flags {
      pub fn new(value: u64, format: &'static str) -> Flags {
            Flags { value, format }
      }
}

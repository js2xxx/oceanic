//! Flags output
//!
//! In the kernel's development, we need to check a series of flags in integers
//! sometimes. A nice output can simplifies the debugging process.
//!
//! See [`Flags`] for more.

use core::{
    fmt::{Display, Error, Formatter},
    str,
};

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

impl Display for Flags {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        let mut buf = [0; 128];
        unsafe {
            for (i, word) in self.format.split_whitespace().enumerate() {
                let buf = &mut buf[0..word.len()];

                buf.copy_from_slice(word.as_bytes());
                let out = str::from_utf8_unchecked_mut(buf);

                let b = (self.value >> i) & 1;
                if b != 0 {
                    out.make_ascii_uppercase();
                } else {
                    out.make_ascii_lowercase();
                }
                write!(f, "{out} ")?;
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

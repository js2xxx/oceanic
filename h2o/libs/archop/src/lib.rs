#![no_std]
#![feature(asm)]

cfg_if::cfg_if! {
      if #[cfg(target_arch = "x86_64")] {
            pub mod x86_64;
            pub use self::x86_64::*;
      }
}

pub mod io;

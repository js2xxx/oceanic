pub mod intr;
pub mod time;

use bitvec::prelude::*;

cfg_if::cfg_if! {
      if #[cfg(target_arch = "x86_64")] {
            #[path = "cpu/x86_64/mod.rs"]
            pub mod arch;
            pub use arch::{id, set_id, count, is_bsp, CPU_INDEX, MAX_CPU};
      }
}

pub fn all_mask() -> CpuMask {
    let mut arr = bitarr![0; MAX_CPU];
    arr[0..count()].set_all(true);
    arr
}

pub fn current_mask() -> CpuMask {
    let mut arr = bitarr![0; MAX_CPU];
    arr.set(unsafe { id() }, true);
    arr
}

pub type CpuMask = BitArr!(for MAX_CPU);

pub mod intr;
pub mod time;

cfg_if::cfg_if! {
      if #[cfg(target_arch = "x86_64")] {
            #[path = "cpu/x86_64/mod.rs"]
            pub mod arch;
      }
}

pub use arch::{id, set_id, CPU_INDEX, MAX_CPU};

pub type CpuMask = bitvec::BitArr!(for MAX_CPU);

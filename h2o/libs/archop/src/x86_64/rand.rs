use bitop_ex::BitOpEx;

use spin::Lazy;

static RAND_AVAILABLE: Lazy<bool> = Lazy::new(|| {
      let cpuid = raw_cpuid::CpuId::new();
      let fi = cpuid.get_feature_info();
      fi.map_or(false, |fi| fi.has_rdrand())
});

pub fn get() -> u64 {
      if *RAND_AVAILABLE {
            for _ in 0..10 {
                  let ret;
                  let flags: u64;
                  unsafe {
                        asm!(
                              "rdrand {}",
                              "pushfq",
                              "pop {}", 
                              out(reg) ret, 
                              out(reg) flags
                        );
                        if flags.contains_bit(crate::reg::rflags::CF) {
                              return ret;
                        }
                  }
            }
      }

      let ret = crate::msr::rdtsc();
      ret.wrapping_mul(0xc345c6b72fd16123)
}

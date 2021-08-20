use spin::Lazy;

#[derive(Debug, Clone, Copy)]
enum FpuType {
      Fn,
      Fx,
      X(u32, u32),
}

static FPU_TYPE: Lazy<FpuType> = Lazy::new(|| {
      let cpuid = raw_cpuid::CpuId::new();
      match cpuid.get_feature_info() {
            Some(e) if e.has_xsave() => {
                  let res1 = raw_cpuid::native_cpuid::cpuid_count(0xD, 0);
                  let res2 = raw_cpuid::native_cpuid::cpuid_count(0xD, 1);
                  FpuType::X(res1.eax | res2.ecx, res1.edx | res2.edx)
            }
            Some(e) if e.has_fxsave_fxstor() => FpuType::Fx,
            _ => FpuType::Fn,
      }
});

/// Save the current state of the x87 FPU into the pointer buffer.
///
/// # Safety
///
/// The caller must ensure that the pointer buffer is enough and the x87 FPU is in a valid state.
pub unsafe fn save(ptr: *mut u8) {
      match *FPU_TYPE {
            // The `fnsave` opcode clears FPU registers, so we reload them to maintain the state.
            FpuType::Fn => asm!("fnsave [{}]; fwait; frstor [{0}]", in(reg) ptr, options(nostack)),
            FpuType::Fx => asm!("fxsave64 [{}]", in(reg) ptr, options(nostack)),
            FpuType::X(ml, mh) => asm!(
                  "xsave [{}]",
                  in(reg) ptr,
                  in("eax") ml,
                  in("edx") mh,
                  options(nostack)
            ),
      }
}

/// Load the current state of the x87 FPU from the pointer buffer.
///
/// # Safety
///
/// The caller must ensure that the pointer buffer is enough and the x87 FPU is in a valid state.
pub unsafe fn load(ptr: *const u8) {
      match *FPU_TYPE {
            FpuType::Fn => asm!("frstor [{}]", in(reg) ptr, options(nostack)),
            FpuType::Fx => asm!("fxrstor64 [{}]", in(reg) ptr, options(nostack)),
            FpuType::X(ml, mh) => asm!(
                  "xrstor [{}]",
                  in(reg) ptr,
                  in("eax") ml,
                  in("edx") mh,
                  options(nostack)
            ),
      }
}

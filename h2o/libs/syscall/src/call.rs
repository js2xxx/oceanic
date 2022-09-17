#![allow(unused_unsafe)]
#![allow(clippy::missing_safety_doc)]

pub(crate) mod hdl;
#[cfg(all(not(feature = "stub"), feature = "call"))]
mod raw;
pub(crate) mod reg;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub struct Syscall {
    pub num: usize,
    pub args: [usize; 5],
}

#[cfg(all(not(feature = "stub"), feature = "call"))]
use crate::{
    c_ty::*,
    ipc::RawPacket,
    mem::{Flags, MemInfo, VirtMapInfo},
    res::IntrConfig,
    task::ExecInfo,
    Feature, Handle, SerdeReg,
};

#[cfg(feature = "vdso")]
#[no_mangle]
pub unsafe extern "C" fn sv_time_get(ptr: *mut ()) -> crate::c_ty::Status {
    let ticks = {
        let (eax, edx): (u32, u32);
        core::arch::asm!("rdtsc", out("eax")eax, out("edx")edx);
        ((edx as u64) << 32) | (eax as u64)
    };

    let c = crate::constants();

    let val = ticks - c.ticks_offset;
    let ns = (val as u128 * c.ticks_multiplier) >> c.ticks_shift;

    ptr.cast::<u128>().write(ns);

    Status::from_res(Ok(()))
}

#[cfg(feature = "vdso")]
#[no_mangle]
pub extern "C" fn sv_random() -> crate::c_ty::StatusOrValue {
    let c = crate::constants();
    if c.has_builtin_rand {
        for _ in 0..10 {
            let ret;
            let flags: u64;
            unsafe {
                core::arch::asm!(
                      "rdrand {}",
                      "pushfq",
                      "pop {}",
                      out(reg) ret,
                      out(reg) flags
                );
                if flags & 1 != 0 {
                    return crate::c_ty::StatusOrValue::from_res(Ok(ret));
                }
            }
        }
    }

    // Fall back to time-based randomization.
    let ticks = unsafe {
        let (eax, edx): (u32, u32);
        core::arch::asm!("rdtsc", out("eax")eax, out("edx")edx);
        ((edx as u64) << 32) | (eax as u64)
    };
    let ret = ticks.wrapping_mul(0xb7123c2fd16c6345);
    crate::c_ty::StatusOrValue::from_res(Ok(ret))
}

#[cfg(feature = "vdso")]
#[no_mangle]
#[inline(never)]
pub extern "C" fn sv_cpu_num() -> crate::c_ty::StatusOrValue {
    crate::c_ty::StatusOrValue::from_res(Ok(crate::constants().num_cpus as u64))
}

#[cfg(all(not(feature = "stub"), feature = "call"))]
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/target/call.rs"));

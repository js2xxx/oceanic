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

#[cfg(all(not(feature = "stub"), feature = "call"))]
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/target/call.rs"));

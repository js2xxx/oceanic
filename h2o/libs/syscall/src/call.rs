#![allow(unused_unsafe)]
#![allow(clippy::missing_safety_doc)]

pub(crate) mod hdl;
#[cfg(all(not(feature = "stub"), feature = "call"))]
mod raw;
pub(crate) mod reg;

#[cfg(all(not(feature = "stub"), feature = "call"))]
use crate::{
    c_ty::*,
    ipc::RawPacket,
    mem::{Flags, MapInfo, MemInfo},
    res::IntrConfig,
    task::ExecInfo,
    Handle, SerdeReg,
};

#[cfg(all(not(feature = "stub"), feature = "call"))]
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/target/call.rs"));

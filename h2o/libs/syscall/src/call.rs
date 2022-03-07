pub(crate) mod hdl;
#[cfg(feature = "call")]
mod raw;
pub(crate) mod reg;

#[cfg(feature = "call")]
use crate::{
    c_ty::*,
    ipc::RawPacket,
    mem::{Flags, MapInfo, MemInfo},
    res::IntrConfig,
    task::ExecInfo,
    Handle, SerdeReg,
};

#[cfg(feature = "call")]
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/target/call.rs"));

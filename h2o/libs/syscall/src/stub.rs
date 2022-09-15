#[cfg(feature = "stub")]
use crate::{
    c_ty::*,
    ipc::RawPacket,
    mem::{Flags, MemInfo, VirtMapInfo},
    res::IntrConfig,
    task::ExecInfo,
    Feature, Handle, Syscall,
};

#[cfg(feature = "stub")]
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/target/stub.rs"));

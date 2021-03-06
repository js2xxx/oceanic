#[cfg(feature = "stub")]
use crate::{
    c_ty::*,
    ipc::RawPacket,
    mem::{Flags, MemInfo, VirtMapInfo},
    res::IntrConfig,
    task::ExecInfo,
    Feature, Handle,
};

#[cfg(feature = "stub")]
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/target/stub.rs"));

use crate::{c_ty::*, ipc::RawPacket, mem::*, res::*, task::ExecInfo, Feature, Handle, Syscall};

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/target/stub.rs"));

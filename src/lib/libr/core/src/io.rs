pub use solvent::mem::{IoSlice, IoSliceMut};
use solvent::prelude::Phys;
use solvent_rpc::SerdePacket;
use solvent_rpc_core as solvent_rpc;

#[derive(SerdePacket)]
pub struct RawStream {
    pub phys: Phys,
    pub len: usize,
    pub seeker: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SerdePacket)]
pub enum SeekFrom {
    Start(usize),
    Current(isize),
    End(isize),
}

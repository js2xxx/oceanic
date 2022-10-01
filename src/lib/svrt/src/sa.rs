use alloc::{collections::BTreeMap, vec::Vec};

use modular_bitfield::{bitfield, BitfieldSpecifier};
use solvent::prelude::{Handle, Object, Phys, Virt};
use solvent_rpc::{
    packet::{Deserializer, SerdePacket, Serializer},
    Error, SerdePacket,
};
use solvent_rpc_core as solvent_rpc;

#[derive(Debug, Copy, Clone, BitfieldSpecifier)]
#[repr(u16)]
#[bits = 16]
pub enum HandleType {
    None = 0,
    RootVirt,
    VdsoPhys,
    ProgramPhys,
    LoadRpc,
    BootfsPhys,
}

#[derive(Copy, Clone)]
#[bitfield]
#[repr(C)]
pub struct HandleInfo {
    pub handle_type: HandleType,
    pub additional: u16,
}

impl SerdePacket for HandleInfo {
    #[inline]
    fn serialize(self, ser: &mut Serializer) -> Result<(), Error> {
        self.bytes.serialize(ser)
    }

    #[inline]
    fn deserialize(de: &mut Deserializer) -> Result<Self, Error> {
        SerdePacket::deserialize(de).map(Self::from_bytes)
    }
}

impl From<HandleType> for HandleInfo {
    #[inline]
    fn from(ty: HandleType) -> Self {
        Self::new().with_handle_type(ty)
    }
}

impl PartialEq for HandleInfo {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl Eq for HandleInfo {}

impl PartialOrd for HandleInfo {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.bytes.partial_cmp(&other.bytes)
    }
}

impl Ord for HandleInfo {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.bytes.cmp(&other.bytes)
    }
}

pub const STARTUP_ARGS: usize = 0x1873ddab8;

#[derive(SerdePacket)]
pub struct StartupArgs {
    pub handles: BTreeMap<HandleInfo, Handle>,
    pub args: Vec<u8>,
    pub env: Vec<u8>,
}

impl StartupArgs {
    pub fn root_virt(&mut self) -> Option<Virt> {
        let handle = self.handles.remove(&HandleType::RootVirt.into())?;
        Some(unsafe { Virt::from_raw(handle) })
    }

    pub fn vdso_phys(&mut self) -> Option<Phys> {
        let handle = self.handles.remove(&HandleType::VdsoPhys.into())?;
        Some(unsafe { Phys::from_raw(handle) })
    }
}

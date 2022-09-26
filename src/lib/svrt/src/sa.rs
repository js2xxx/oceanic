use alloc::{collections::BTreeMap, vec::Vec};

use solvent::prelude::{Error, Handle, Object, Phys, Virt, EBUFFER, ETYPE};
use solvent_rpc::{packet::Method, SerdePacket};
use solvent_rpc_core as solvent_rpc;

use crate::{HandleInfo, HandleType};

#[derive(Debug)]
pub enum TryFromError {
    SignatureMismatch([u8; 4]),
    BufferTooShort(usize),
    Other(Error),
}

impl From<TryFromError> for Error {
    fn from(val: TryFromError) -> Self {
        match val {
            TryFromError::SignatureMismatch(_) => ETYPE,
            TryFromError::BufferTooShort(_) => EBUFFER,
            TryFromError::Other(err) => err,
        }
    }
}

#[derive(SerdePacket)]
pub struct StartupArgs {
    pub handles: BTreeMap<HandleInfo, Handle>,
    pub args: Vec<u8>,
    pub env: Vec<u8>,
}

impl Method for StartupArgs {
    const METHOD_ID: usize = 0x1873ddab8;
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

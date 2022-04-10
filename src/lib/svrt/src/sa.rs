use alloc::{collections::BTreeMap, vec::Vec};
use core::{array::TryFromSliceError, mem};

use solvent::prelude::{Error, Handle, Object, Packet, PacketTyped, Phys, Virt};

use crate::{
    HandleInfo, HandleType, StartupArgsHeader, PACKET_SIG_STARTUP_ARGS, STARTUP_ARGS_HEADER_SIZE,
};

#[derive(Debug)]
pub enum TryFromError {
    SignatureMismatch([u8; 4]),
    StringParseError(cstr_core::FromBytesWithNulError),
    BufferTooShort,
    Other(Error),
}

impl From<TryFromSliceError> for TryFromError {
    fn from(_: TryFromSliceError) -> Self {
        Self::BufferTooShort
    }
}

impl From<cstr_core::FromBytesWithNulError> for TryFromError {
    fn from(err: cstr_core::FromBytesWithNulError) -> Self {
        Self::StringParseError(err)
    }
}

impl From<TryFromError> for Error {
    fn from(val: TryFromError) -> Self {
        match val {
            TryFromError::SignatureMismatch(_) => Error::ETYPE,
            TryFromError::StringParseError(_) => Error::ETYPE,
            TryFromError::BufferTooShort => Error::EBUFFER,
            TryFromError::Other(err) => err,
        }
    }
}

pub struct StartupArgs {
    pub handles: BTreeMap<HandleInfo, Handle>,
    pub args: Vec<u8>,
    pub env: Vec<u8>,
}

impl StartupArgs {
    pub fn root_virt(&mut self) -> Option<Virt> {
        let handle = self
            .handles
            .remove(&HandleInfo::new().with_handle_type(HandleType::RootVirt))?;
        Some(unsafe { Virt::from_raw(handle) })
    }

    pub fn vdso_phys(&mut self) -> Option<Phys> {
        let handle = self
            .handles
            .remove(&HandleInfo::new().with_handle_type(HandleType::VdsoPhys))?;
        Some(unsafe { Phys::from_raw(handle) })
    }
}

impl PacketTyped for StartupArgs {
    type TryFromError = TryFromError;

    fn into_packet(self) -> Packet {
        let (mut hinfos, handles) = self.handles.into_iter().fold(
            (Vec::new(), Vec::new()),
            |(mut acci, mut acch), (info, hdl)| {
                acci.extend_from_slice(&info.into_bytes());
                acch.push(hdl);
                (acci, acch)
            },
        );

        let mut args = self.args;

        let mut env = self.env;

        let handle_info_offset = STARTUP_ARGS_HEADER_SIZE;
        let args_offset = handle_info_offset + args.len();
        let env_offset = args_offset + env.len();

        let header = StartupArgsHeader {
            signature: PACKET_SIG_STARTUP_ARGS,
            handle_info_offset,
            handle_count: handles.len(),
            args_offset,
            args_len: args.len(),
            env_offset,
            env_len: env.len(),
        };

        let mut buffer = Vec::from(header.as_bytes());
        buffer.append(&mut hinfos);
        buffer.append(&mut args);
        buffer.append(&mut env);

        Packet {
            buffer,
            handles,
            ..Default::default()
        }
    }

    fn try_from_packet(packet: &mut Packet) -> Result<Self, TryFromError> {
        let header = { packet.buffer.get(..STARTUP_ARGS_HEADER_SIZE) }
            .and_then(StartupArgsHeader::from_bytes)
            .ok_or(TryFromError::BufferTooShort)?;
        if header.signature != PACKET_SIG_STARTUP_ARGS {
            return Err(TryFromError::SignatureMismatch(header.signature));
        }

        let handles = packet
            .buffer
            .get(header.handle_info_offset..)
            .and_then(|data| data.get(..header.handle_count * mem::size_of::<HandleInfo>()))
            .ok_or(TryFromError::BufferTooShort)?
            .chunks(mem::size_of::<HandleInfo>())
            .map(|slice| slice.try_into().map(HandleInfo::from_bytes))
            .zip(packet.handles.iter())
            .map(|(info, &handle)| info.map(|info| (info, handle)))
            .try_collect::<BTreeMap<_, _>>()?;

        let args = Vec::from(
            packet
                .buffer
                .get(header.args_offset..)
                .and_then(|data| data.get(..header.args_len))
                .ok_or(TryFromError::BufferTooShort)?,
        );

        let env = Vec::from(
            packet
                .buffer
                .get(header.env_offset..)
                .and_then(|data| data.get(..header.env_len))
                .ok_or(TryFromError::BufferTooShort)?,
        );

        *packet = Default::default();

        Ok(StartupArgs {
            handles,
            args,
            env,
        })
    }
}

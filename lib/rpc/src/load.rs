use alloc::vec::Vec;
use core::mem;

use cstr_core::CString;
use solvent::prelude::{Error, Object, Packet, PacketTyped, Phys};

use crate::{from_cstr_vec, parse_cstr_vec, Byted, Carrier};

pub struct GetObject;

impl Carrier for GetObject {
    type Request = GetObjectRequest;
    type Response = GetObjectResponse;
}

pub struct GetObjectRequest {
    pub paths: Vec<CString>,
}

impl From<Vec<CString>> for GetObjectRequest {
    fn from(paths: Vec<CString>) -> Self {
        GetObjectRequest { paths }
    }
}

#[derive(Default)]
#[repr(C)]
pub struct GetObjectRequestHeader {
    pub sig: [u8; 4],
    pub path_len: usize,
    pub path_offset: usize,
}
unsafe impl Byted for GetObjectRequestHeader {}
pub const PACKET_SIG_LOAD_LIBRARY_REQUEST: [u8; 4] = [0xaf, 0x49, 0x01, 0x45];

impl PacketTyped for GetObjectRequest {
    type TryFromError = Error;

    fn into_packet(self) -> Packet {
        let mut paths = from_cstr_vec(self.paths);
        let header = GetObjectRequestHeader {
            sig: PACKET_SIG_LOAD_LIBRARY_REQUEST,
            path_len: paths.len(),
            path_offset: mem::size_of::<GetObjectRequestHeader>(),
        };
        let mut buffer = Vec::from(header.as_bytes());
        buffer.append(&mut paths);
        Packet {
            buffer,
            ..Default::default()
        }
    }

    fn try_from_packet(packet: &mut Packet) -> Result<Self, Self::TryFromError> {
        let header = packet
            .buffer
            .get(..mem::size_of::<GetObjectRequestHeader>())
            .and_then(GetObjectRequestHeader::from_bytes)
            .ok_or(Error::EBUFFER)?;
        if header.sig != PACKET_SIG_LOAD_LIBRARY_REQUEST {
            return Err(Error::ETYPE);
        }
        let paths = { packet.buffer.get(header.path_offset..) }
            .and_then(|s| s.get(..header.path_len))
            .and_then(|s| parse_cstr_vec(s).ok())
            .ok_or(Error::EBUFFER)?;

        *packet = Default::default();
        Ok(GetObjectRequest { paths })
    }
}

pub enum GetObjectResponse {
    Error { not_found_index: usize },
    Success(Vec<Phys>),
}

#[derive(Default)]
#[repr(C)]
pub struct GetObjectResponseHeader {
    pub is_ok: bool,
    pub not_found_index: usize,
    pub handle_count: usize,
}
unsafe impl Byted for GetObjectResponseHeader {}

impl PacketTyped for GetObjectResponse {
    type TryFromError = Error;

    fn into_packet(self) -> Packet {
        match self {
            GetObjectResponse::Error { not_found_index } => {
                let header = GetObjectResponseHeader {
                    is_ok: false,
                    not_found_index,
                    ..Default::default()
                };
                let buffer = Vec::from(header.as_bytes());
                Packet {
                    buffer,
                    ..Default::default()
                }
            }
            GetObjectResponse::Success(objs) => {
                let handles = objs.into_iter().map(Phys::into_raw).collect::<Vec<_>>();
                let header = GetObjectResponseHeader {
                    is_ok: true,
                    handle_count: handles.len(),
                    ..Default::default()
                };
                let buffer = Vec::from(header.as_bytes());
                Packet {
                    buffer,
                    handles,
                    ..Default::default()
                }
            }
        }
    }

    fn try_from_packet(packet: &mut Packet) -> Result<Self, Self::TryFromError> {
        let header = packet
            .buffer
            .get(..mem::size_of::<GetObjectResponseHeader>())
            .and_then(GetObjectResponseHeader::from_bytes)
            .ok_or(Error::EBUFFER)?;
        if header.is_ok {
            if packet.handles.len() != header.handle_count {
                return Err(Error::ETYPE);
            }
            let objs = packet
                .handles
                .iter()
                .map(|&handle| unsafe { Phys::from_raw(handle) })
                .collect();

            Ok(GetObjectResponse::Success(objs))
        } else {
            if !packet.handles.is_empty() {
                return Err(Error::ETYPE);
            }
            Ok(GetObjectResponse::Error {
                not_found_index: header.not_found_index,
            })
        }
    }
}

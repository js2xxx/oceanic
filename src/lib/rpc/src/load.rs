use alloc::{ffi::CString, vec::Vec};

use solvent::prelude::Phys;
use solvent_rpc::{packet::Method, SerdePacket};

use crate::Carrier;

pub struct GetObject;

impl Carrier for GetObject {
    type Request = GetObjectRequest;
    type Response = GetObjectResponse;
}

#[derive(SerdePacket)]
pub struct GetObjectRequest {
    pub paths: Vec<CString>,
}

impl Method for GetObjectRequest {
    const METHOD_ID: usize = 0x172386ab2733;
}

impl From<Vec<CString>> for GetObjectRequest {
    fn from(paths: Vec<CString>) -> Self {
        GetObjectRequest { paths }
    }
}

#[derive(SerdePacket)]
pub enum GetObjectResponse {
    Error { not_found_index: usize },
    Success(Vec<Phys>),
}

impl Method for GetObjectResponse {
    const METHOD_ID: usize = 0x293847238ac;
}

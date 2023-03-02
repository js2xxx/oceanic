use alloc::{string::String, vec::Vec};

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[derive(Encode, Decode)]
pub struct Component {
    pub header: Header,
}

#[derive(Debug, Serialize, Deserialize)]
#[derive(Encode, Decode)]
#[serde(tag = "type")]
pub enum Header {
    #[serde(rename = "binary")]
    Binary(Binary),
    #[serde(rename = "driver")]
    Driver(Driver),
}

#[derive(Debug, Serialize, Deserialize)]
#[derive(Encode, Decode)]
pub struct Binary {
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[derive(Encode, Decode)]
pub struct Driver {
    pub path: String,
    pub matches: Vec<String>,
}

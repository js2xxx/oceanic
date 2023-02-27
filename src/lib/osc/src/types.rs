use alloc::{string::String, vec::Vec};

use serde::{Deserialize, Serialize};


#[derive(Serialize, Deserialize)]
pub struct Component {
    pub header: Header,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Header {
    #[serde(rename = "binary")]
    Binary(Binary),
    #[serde(rename = "driver")]
    Driver(Driver),
}

#[derive(Serialize, Deserialize)]
pub struct Binary {
    pub path: String,
}

#[derive(Serialize, Deserialize)]
pub struct Driver {
    pub path: String,
    pub matches: Vec<String>,
}

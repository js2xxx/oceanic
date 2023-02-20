use alloc::{string::String, vec::Vec};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct DriverConfig {
    pub matches: Vec<String>,
}

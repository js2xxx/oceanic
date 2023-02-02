use solvent::mem::Phys;

use crate as solvent_rpc;

#[protocol]
pub trait Driver: crate::core::Closeable {}

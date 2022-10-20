use crate as solvent_rpc;
cfg_if::cfg_if! {
    if #[cfg(feature = "std")] {
        use alloc::{ffi::CString, vec::Vec};

        use solvent::prelude::Phys;
    }
}

/// The loader service interface.
#[protocol]
pub trait Loader {
    /// Acquire a set of objects indexed by its path from the loader service
    /// provider.
    ///
    /// # Returns
    ///
    /// The physical kernel object of the acquired objects respectively.
    ///
    /// # Errors
    ///
    /// If one of the acquired objects is not found, then its index is returned.
    fn get_object(path: Vec<CString>) -> Result<Vec<Phys>, usize>;
}

pub use loader::*;

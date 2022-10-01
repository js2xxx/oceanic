cfg_if::cfg_if! {
    if #[cfg(feature = "std")] {
        use alloc::{ffi::CString, vec::Vec};

        use solvent::prelude::Phys;

        use crate as solvent_rpc;
    }
}

/// The loader service interface.
#[crate::protocol]
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
    #[id(0x172386ab2733)]
    async fn get_object(path: Vec<CString>) -> Result<Vec<Phys>, usize>;
}

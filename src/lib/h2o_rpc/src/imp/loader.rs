cfg_if::cfg_if! {
    if #[cfg(feature = "std")] {
        use alloc::{ffi::CString, vec::Vec};

        use solvent::prelude::Phys;

        use crate as solvent_rpc;
    }
}

#[crate::protocol]
pub trait Loader {
    #[id(0x172386ab2733)]
    async fn get_object(path: Vec<CString>) -> Result<Vec<Phys>, usize>;
}
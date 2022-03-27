use super::Virt;
use crate::{error::Result, obj::Object};

#[repr(transparent)]
pub struct Space(sv_call::Handle);

crate::impl_obj!(Space);
crate::impl_obj!(@DROP, Space);

impl Space {
    pub fn try_new() -> Result<(Self, Virt)> {
        let mut root_virt = sv_call::Handle::NULL;
        let handle = unsafe { sv_call::sv_space_new(&mut root_virt).into_res() }?;
        // SAFETY: The handle is freshly allocated.
        Ok(unsafe { (Self::from_raw(handle), Virt::from_raw(root_virt)) })
    }

    pub fn new() -> (Self, Virt) {
        Self::try_new().expect("Failed to create task space")
    }
}

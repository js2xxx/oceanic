use solvent_fs::fs::LocalFs;

#[inline]
pub fn local() -> &'static LocalFs {
    crate::ffi::local_fs()
}

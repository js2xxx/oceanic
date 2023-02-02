use alloc::alloc::Global;
use core::{
    alloc::{Allocator, Layout},
    ptr::NonNull,
};

use solvent_async::{global_executor, local_executor};
use solvent_ddk::ffi::VTable;
use solvent_fs::fs;

#[no_mangle]
unsafe extern "C" fn __h2o_ddk_alloc(size: usize, align: usize) -> *mut () {
    let layout = Layout::from_size_align(size, align).unwrap();
    Global
        .allocate(layout)
        .map_or(core::ptr::null_mut(), |ptr| ptr.as_ptr().cast())
}

#[no_mangle]
unsafe extern "C" fn __h2o_ddk_dealloc(ptr: *mut (), size: usize, align: usize) {
    if let (Some(ptr), Ok(layout)) = (
        NonNull::new(ptr.cast()),
        Layout::from_size_align(size, align),
    ) {
        Global.deallocate(ptr, layout)
    }
}

pub fn vtable() -> VTable {
    VTable {
        global_exe: global_executor() as _,
        local_exe: local_executor(|exe| exe as *const _),
        local_fs: fs::local() as *const _,

        alloc: __h2o_ddk_alloc,
        dealloc: __h2o_ddk_dealloc,
    }
}

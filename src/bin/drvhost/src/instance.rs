use alloc::{boxed::Box, ffi::CString};
use core::{
    error::Error,
    ffi::{c_char, c_void, CStr},
    future::Future,
    ptr,
};

use async_task::Task;
use solvent::prelude::{Channel, Handle, Object, Phys};
use solvent_fs::fs;
use solvent_rpc::io::{
    file::{FileSyncClient, PhysOptions},
    OpenOptions,
};
use solvent_std::{c_str, path::Path};

pub fn bootstrap(file_path: &Path) -> Result<impl Future<Output = ()>, Box<dyn Error>> {
    let (driver, dserver) = Channel::new();
    fs::local().open("use/devm", OpenOptions::READ | OpenOptions::WRITE, dserver)?;

    let (file, fserver) = Channel::new();
    fs::local().open(
        file_path,
        OpenOptions::READ | OpenOptions::EXECUTE | OpenOptions::EXPECT_FILE,
        fserver,
    )?;
    let file = FileSyncClient::from(file);
    let phys = file.phys(PhysOptions::Shared)??;

    let name = CString::new(file_path.to_str().unwrap())?;
    create(driver, phys, &name)
}

fn create(
    driver: Channel,
    phys: Phys,
    name: &CStr,
) -> Result<impl Future<Output = ()>, Box<dyn Error>> {
    // Load the DSO.
    let dso = {
        let phys = Phys::into_raw(phys);
        unsafe { dlphys(phys, name.as_ptr()) }
    };

    // Get `__h2o_ddk_enter` function.
    let ddk_enter =
        unsafe { ddk_fn::<Enter>(dso, c_str!("__h2o_ddk_enter")) }.ok_or("ddk_enter not found")?;

    // And `__h2o_ddk_exit`.
    let ddk_exit =
        unsafe { ddk_fn::<Exit>(dso, c_str!("__h2o_ddk_exit")) }.ok_or("ddk_exit not found")?;

    // Initialize the driver environment.
    let task = unsafe {
        let ptr = ddk_enter(&crate::ffi::vtable() as _, Channel::into_raw(driver));
        Box::from_raw(ptr.cast::<Task<()>>())
    };

    Ok(async move {
        task.await;
        unsafe { ddk_exit() };
    })
}

#[link(name = "ldso")]
extern "C" {
    fn dlphys(phys: Handle, name: *const c_char) -> *const c_void;

    fn dlsym(handle: *const c_void, name: *const c_char) -> *mut c_void;
}

/// # Safety
///
/// `F` must be a static `fn` type and the same signature with the
/// definition.
unsafe fn ddk_fn<F>(dso: *const c_void, name: &CStr) -> Option<F> {
    let func = dlsym(dso, name.as_ptr());
    if func.is_null() {
        return None;
    }
    Some(ptr::read(&func as *const _ as *const F))
}

type Enter = unsafe extern "C" fn(
    vtable: *const solvent_ddk::ffi::VTable,
    instance: solvent::obj::Handle,
) -> *mut ();

type Exit = unsafe extern "C" fn();

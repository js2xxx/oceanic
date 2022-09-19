use alloc::vec::Vec;
use core::{
    hint,
    mem::{self, MaybeUninit},
    sync::atomic::{AtomicUsize, Ordering::*},
};

use solvent::{
    c_ty::{StatusOrHandle, StatusOrValue},
    prelude::{Handle, Object, Ref, Result, Virt, EEXIST, ENOENT},
};
use spin::Mutex;

use crate::{HandleInfo, StartupArgs};

static STARTUP_LOCK: Mutex<()> = Mutex::new(());

static STARTUP_STATE: AtomicUsize = AtomicUsize::new(0);

static mut STARTUP_ARGS: MaybeUninit<StartupArgs> = MaybeUninit::uninit();
static mut ROOT_VIRT: Option<Virt> = None;
static mut ENVS: &[u8] = &[];

const SS_UNINIT: usize = 0;
const SS_PROGRESS: usize = 1;
const SS_INIT: usize = 2;

pub fn init_rt(args: StartupArgs) -> Result<Vec<u8>> {
    loop {
        let value = STARTUP_STATE.load(Acquire);
        match value {
            SS_INIT => break Err(EEXIST),
            SS_PROGRESS => hint::spin_loop(),
            SS_UNINIT => {
                match STARTUP_STATE.compare_exchange(SS_UNINIT, SS_PROGRESS, Acquire, Acquire) {
                    Err(_) => hint::spin_loop(),
                    Ok(_) => {
                        let args = unsafe {
                            let args = STARTUP_ARGS.write(args);
                            ROOT_VIRT = args.root_virt();
                            ENVS = &args.env;
                            mem::take(&mut args.args)
                        };
                        STARTUP_STATE.store(SS_INIT, Release);
                        break Ok(args);
                    }
                }
            }
            _ => panic!("Poisoned start-up state: {:?}", value),
        }
    }
}

#[inline]
fn init_or<F, R>(func: F) -> Result<R>
where
    F: FnOnce() -> Result<R>,
{
    if STARTUP_STATE.load(Acquire) == SS_INIT {
        func()
    } else {
        Err(ENOENT)
    }
}

pub fn try_get_root_virt() -> Result<Ref<'static, Virt>> {
    init_or(|| unsafe { ROOT_VIRT.as_ref().map(Into::into).ok_or(ENOENT) })
}

pub fn root_virt() -> Ref<'static, Virt> {
    try_get_root_virt().expect("Failed to get the root virt: uninitialized or failed to receive")
}

/// # Safety
///
/// The caller must ensure that the ownership of the root virt is not
/// transferred.
#[no_mangle]
pub unsafe extern "C" fn sv_root_virt() -> StatusOrHandle {
    StatusOrHandle::from_res(try_get_root_virt().map(|virt| virt.raw()))
}

/// Note: The ownership of the handle is transferred if successful.
pub fn try_take_startup_handle(info: HandleInfo) -> Result<Handle> {
    init_or(|| unsafe {
        let _lock = STARTUP_LOCK.lock();
        STARTUP_ARGS
            .assume_init_mut()
            .handles
            .remove(&info)
            .ok_or(ENOENT)
    })
}

/// Note: The ownership of the handle is transferred.
pub fn take_startup_handle(info: HandleInfo) -> Handle {
    try_take_startup_handle(info).expect(
        "Failed to take the startup handle: uninitialized, failed to receive or already taken",
    )
}

/// Note: The ownership of the handle is transferred if successful.
#[no_mangle]
pub extern "C" fn sv_take_startup_handle(info: HandleInfo) -> StatusOrHandle {
    StatusOrHandle::from_res(try_take_startup_handle(info))
}

pub fn try_get_envs() -> Result<&'static [u8]> {
    init_or(|| Ok(unsafe { ENVS }))
}

pub fn envs() -> &'static [u8] {
    try_get_envs().expect("Failed to get environment variables: uninitialized")
}

#[no_mangle]
pub extern "C" fn sv_get_envs() -> StatusOrValue {
    StatusOrValue::from_res(try_get_envs().map(|envs| envs.as_ptr() as u64))
}

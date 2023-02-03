use alloc::{
    borrow::ToOwned,
    string::{String, ToString},
};

use solvent_core::ffi::{OsStr, OsString};

use crate::rt::ARGS;

pub fn args() -> impl Iterator<Item = String> {
    args_os().map(|s| s.to_str().unwrap().to_string())
}

pub fn args_os() -> impl Iterator<Item = &'static OsStr> {
    unsafe {
        ARGS.split(|&b| b == 0)
            .filter(|s| s != &[])
            .map(OsStr::from_bytes)
    }
}

pub fn vars_os() -> impl Iterator<Item = (OsString, OsString)> {
    svrt::envs().split(|&b| b == 0).filter_map(|s| {
        let pos = memchr::memchr(b'=', s)?;
        let (key, value) = s.split_at(pos);
        Some((
            OsString::from_vec(key.to_owned()),
            OsString::from_vec(value[1..].to_owned()),
        ))
    })
}

pub fn vars() -> impl Iterator<Item = (String, String)> {
    vars_os().map(|(key, value)| (key.into_string().unwrap(), value.into_string().unwrap()))
}

#[panic_handler]
fn rust_begin_unwind(info: &core::panic::PanicInfo) -> ! {
    log::error!("{}", info);

    loop {
        unsafe { core::arch::asm!("pause; ud2") }
    }
}

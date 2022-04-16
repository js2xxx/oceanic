use core::{
    ffi::{c_char, c_int, c_void},
    ptr,
    sync::atomic::{AtomicPtr, Ordering::SeqCst},
};

use cstr_core::{CStr, CString};
use solvent::{c_ty::Status, prelude::EINVAL};

use crate::{
    dso::{self, dso_list, get_object, Dso},
    elf::Tcb,
};

pub const RTLD_NOW: c_int = 2;

pub const RTLD_LOCAL: c_int = 0;

pub const RTLD_DEFAULT: *const c_void = ptr::null();

static STATUS: AtomicPtr<c_char> = AtomicPtr::new(ptr::null_mut());
fn set_status(res: dso::Error) {
    let val: &'static str = match res {
        dso::Error::SymbolLoad => "Symbol loading error",
        dso::Error::ElfLoad(_) => "Elf loading error",
        dso::Error::DepGet(err) => err.desc(),
        dso::Error::Memory(..) => "Memory exhausted",
    };
    STATUS.store(val.as_ptr() as *mut c_char, SeqCst)
}

fn set_status_str(s: &'static str) {
    STATUS.store(s.as_ptr() as *mut c_char, SeqCst)
}

macro_rules! ok {
    ($res:expr) => {
        match $res {
            Ok(x) => x,
            Err(err) => {
                set_status(err);
                return ptr::null_mut();
            }
        }
    };
    ($cond:expr, $str:literal) => {
        if !$cond {
            set_status_str($str);
            return ptr::null_mut();
        }
    };
    ($cond:expr, $str:literal, $ret:expr) => {
        if !$cond {
            set_status_str($str);
            return $ret;
        }
    };
}

/// # Safety
///
/// The caller must ensure that `path` is a valid c-string.
#[no_mangle]
pub unsafe extern "C" fn dlopen(path: *const c_char, mode: c_int) -> *const c_void {
    ok!(mode == RTLD_NOW, "Load mode not supported");

    let path = CString::from(CStr::from_ptr(path));

    let phys = ok!(get_object([path.clone()].into())).swap_remove(0);
    let (_, dso) = ok!(Dso::load(&phys, path, false));

    dso.as_ptr().cast()
}

/// # Safety
///
/// The caller must ensure that `name` is a valid c-string and `handle` is a
/// valid pointer returned from `dlopen`.
#[no_mangle]
pub unsafe extern "C" fn dlsym(handle: *const c_void, name: *const c_char) -> *mut c_void {
    let ptr = handle as *const Dso;

    let dso = if handle.is_null() {
        None
    } else {
        let canary = unsafe { ptr::read(ptr::addr_of!((*ptr).canary)) };
        ok!(canary.check(), "The handle is invalid");

        Some(unsafe { &*ptr })
    };

    let name = CStr::from_ptr(name);

    match dso_list().lock().get_symbol_value(dso, name) {
        Some(ret) => ret as _,
        None => {
            set_status_str("Symbol not found");
            ptr::null_mut()
        }
    }
}

/// # Safety
///
/// The caller must ensure that `handle` is a valid pointer returned from
/// `dlopen`.
#[no_mangle]
pub extern "C" fn dlclose(handle: *const c_void) -> Status {
    let ptr = handle as *const Dso;
    let canary = unsafe { ptr::read(ptr::addr_of!((*ptr).canary)) };
    ok!(
        canary.check(),
        "The handle is invalid",
        Status::from_res(Err(EINVAL))
    );

    let _ = dso_list().lock().pop(unsafe { &*ptr }.into());

    Status::from_res(Ok(()))
}

#[no_mangle]
pub extern "C" fn dlerror() -> *const c_char {
    STATUS.swap(ptr::null_mut(), SeqCst)
}

#[repr(C)]
struct TlsGetAddr {
    id: usize,
    offset: usize,
}

#[no_mangle]
unsafe extern "C" fn __tls_get_addr(arg: *const TlsGetAddr) -> *mut c_void {
    fn tls_get_addr(id: usize, offset: usize) -> Option<*mut c_void> {
        let list = dso_list().lock();
        let tls = list.tls().get(id)?;
        let chunk = tls.get(unsafe { Tcb::current().index })?;
        chunk.get(offset).map(|r| r as *const _ as *mut _)
    }

    let TlsGetAddr { id, offset } = ptr::read(arg);
    tls_get_addr(id, offset).unwrap_or(ptr::null_mut())
}

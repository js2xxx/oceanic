use alloc::{ffi::CString, vec::Vec};
use core::{
    ffi::{c_char, c_int, c_void, CStr},
    mem, ptr,
    sync::atomic::{AtomicPtr, Ordering::SeqCst},
};

use solvent::{
    c_ty::Status,
    prelude::{Handle, Object, Phys, EINVAL},
};

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
        dso::Error::Serde(..) => "Serde error",
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
    let (_, dso) = ok!(Dso::load(phys, path, false));

    dso.as_ptr().cast()
}

/// # Safety
///
/// The caller must ensure that `phys` is a `Phys` object, and `name` is a valid
/// c-string.
#[no_mangle]
pub unsafe extern "C" fn dlphys(phys: Handle, name: *const c_char) -> *const c_void {
    let phys = Phys::from_raw(phys);
    let name = CStr::from_ptr(name);

    let (_, dso) = ok!(Dso::load(phys, name, false));
    dso.as_ptr().cast()
}

/// # Safety
///
/// The caller must ensure that `name` is a valid c-string and `handle` is a
/// valid pointer returned from `dlopen`.
#[no_mangle]
pub unsafe extern "C" fn dlsym(handle: *const c_void, name: *const c_char) -> *mut c_void {
    let name = CStr::from_ptr(name);

    let ptr = handle as *const Dso;

    let dso = if handle.is_null() {
        None
    } else {
        let canary = unsafe { ptr::read(ptr::addr_of!((*ptr).canary)) };
        ok!(canary.check(), "The handle is invalid");

        Some(unsafe { &*ptr })
    };

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
pub unsafe extern "C" fn dlclose(handle: *const c_void) -> Status {
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

#[no_mangle]
pub extern "C" fn dldisconn() {
    crate::dso::disconnect_ldrpc()
}

#[repr(C)]
pub(crate) struct TlsGetAddr {
    pub id: usize,
    pub offset: usize,
}

#[no_mangle]
pub(crate) unsafe extern "C" fn __tls_get_addr(arg: *const TlsGetAddr) -> *mut c_void {
    fn tls_get_addr(id: usize, offset_in_tls: usize) -> Option<*mut c_void> {
        let mut list = dso_list().lock();
        let offset = list.tls(id)?.offset();
        let ptr = unsafe { Tcb::current().data.get_mut(offset).map(|s| s as *mut u8) }?;
        Some(unsafe { ptr.add(offset_in_tls) as _ })
    }

    let TlsGetAddr { id, offset } = ptr::read(arg);
    tls_get_addr(id, offset).unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn __libc_allocate_tcb() {
    let mut list = dso_list().lock();
    list.push_thread(true);
}

/// # Safety
///
/// `data` and `dtor` must be valid.
#[no_mangle]
pub unsafe extern "C" fn __libc_register_tcb_dtor(data: *mut c_void, dtor: *mut c_void) {
    Tcb::current().dtors.push((data.cast(), dtor.cast()));
}

#[no_mangle]
pub extern "C" fn __libc_deallocate_tcb() {
    unsafe {
        let tcb = Tcb::current();
        let dtors = mem::take(&mut tcb.dtors).into_iter().rev();
        dtors.for_each(|dtor| {
            type Dtor = unsafe extern "C" fn(*mut u8);
            mem::transmute::<_, Dtor>(dtor.1)(dtor.0)
        });
        tcb.static_base = ptr::null_mut();
        tcb.tcb_id = 0;
        tcb.data = Vec::new();
    }
}

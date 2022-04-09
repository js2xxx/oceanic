use core::{ffi::*, ptr, slice};

use bitvec::prelude::*;
use cstr_core::CStr;

/// # Safety
///
/// Check [`CStr::from_ptr`].
/// The caller must ensure that `dest` has a valid `c_char` array with length
/// greater than `src`'s.
#[no_mangle]
pub unsafe extern "C" fn strcpy(dest: *mut c_char, src: *const c_char) -> *mut c_char {
    let (mut d, mut s) = (dest, src);
    while *s != 0 {
        *d = *s;
        d = d.add(1);
        s = s.add(1);
    }
    *d = *s;
    dest
}

/// # Safety
///
/// Check [`CStr::from_ptr`].
/// The caller must ensure that `dest` has a valid `c_char` array with length
/// at least the maximum of `src`'s and `count`.
#[no_mangle]
pub unsafe extern "C" fn strncpy(
    dest: *mut c_char,
    src: *const c_char,
    count: c_size_t,
) -> *mut c_char {
    let mut f = false;
    for i in 0..count {
        if f {
            *dest.add(i) = 0;
        } else {
            let ch = *src.add(i);
            *dest.add(i) = ch;
            f = f || ch == 0;
        }
    }
    dest
}

/// # Safety
///
/// Check [`CStr::from_ptr`].
/// The caller must ensure that `dest` has a c_char array with an additional
/// length at least `src`'s.
#[no_mangle]
pub unsafe extern "C" fn strcat(dest: *mut c_char, src: *const c_char) -> *mut c_char {
    strcpy(dest.add(strlen(dest)), src);
    dest
}

/// # Safety
///
/// Check [`CStr::from_ptr`].
/// The caller must ensure that `dest` has a c_char array with an additional
/// length at least 1 plus the minimum of `src`'s and `count`.
#[no_mangle]
pub unsafe extern "C" fn strncat(
    dest: *mut c_char,
    src: *const c_char,
    count: c_size_t,
) -> *mut c_char {
    let d = dest.add(strlen(dest));
    let count = count.min(strlen(src));
    for i in 0..=count {
        *d.add(i) = *src.add(i);
    }
    dest
}

/// # Safety
///
/// Same as [`strcpy`].
#[no_mangle]
pub unsafe extern "C" fn strxfrm(dest: *mut c_char, src: *const c_char, n: c_size_t) -> c_size_t {
    let len = strlen(src);
    if len < n {
        strcpy(dest, src);
    }
    len
}

/// # Safety
///
/// Same as [`CStr::from_ptr`].
#[no_mangle]
pub unsafe extern "C" fn strlen(s: *const c_char) -> usize {
    let cstr = CStr::from_ptr(s);
    cstr.to_bytes().len()
}

/// # Safety
///
/// Same as [`CStr::from_ptr`].
#[no_mangle]
pub unsafe extern "C" fn strcmp(lhs: *const c_char, rhs: *const c_char) -> c_int {
    strncmp(lhs, rhs, usize::MAX)
}

/// # Safety
///
/// Same as [`CStr::from_ptr`].
#[no_mangle]
pub unsafe extern "C" fn strncmp(lhs: *const c_char, rhs: *const c_char, count: c_size_t) -> c_int {
    let lhs = CStr::from_ptr(lhs).to_bytes();
    let rhs = CStr::from_ptr(rhs).to_bytes();
    let lhs = lhs.get(..count).unwrap_or(lhs);
    let rhs = rhs.get(..count).unwrap_or(rhs);
    match lhs.cmp(rhs) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

/// # Safety
///
/// Same as [`CStr::from_ptr`].
#[no_mangle]
pub unsafe extern "C" fn strcoll(lhs: *const c_char, rhs: *const c_char) -> c_int {
    strcmp(lhs, rhs)
}

/// # Safety
///
/// Same as [`CStr::from_ptr`].
#[no_mangle]
pub unsafe extern "C" fn strchr(s: *const c_char, ch: c_int) -> *const c_char {
    let haystack = CStr::from_ptr(s).to_bytes();
    let pos = memchr::memchr(ch as u8, haystack);
    pos.map_or(ptr::null(), |pos| unsafe { s.add(pos) })
}

unsafe fn strspn_inner(dest: *const c_char, src: *const c_char, cmp: bool) -> c_size_t {
    let dest = CStr::from_ptr(dest).to_bytes();
    let src = CStr::from_ptr(src).to_bytes();

    let mut bytes = bitarr![0; c_char::BITS as usize];
    for &byte in src {
        bytes.set(byte as usize, true);
    }
    for (i, &byte) in dest.iter().enumerate() {
        if bytes[byte as usize] != cmp {
            return i;
        }
    }
    dest.len()
}

/// # Safety
///
/// Same as [`CStr::from_ptr`].
#[no_mangle]
pub unsafe extern "C" fn strspn(dest: *const c_char, src: *const c_char) -> c_size_t {
    strspn_inner(dest, src, true)
}

/// # Safety
///
/// Same as [`CStr::from_ptr`].
#[no_mangle]
pub unsafe extern "C" fn strcspn(dest: *const c_char, src: *const c_char) -> c_size_t {
    strspn_inner(dest, src, false)
}

/// # Safety
///
/// Same as [`CStr::from_ptr`].
#[no_mangle]
pub unsafe extern "C" fn strpbrk(dest: *const c_char, breakset: *const c_char) -> *const c_char {
    let ptr = dest.add(strcspn(dest, breakset));
    if *ptr != 0 {
        ptr
    } else {
        ptr::null()
    }
}

/// # Safety
///
/// Same as [`CStr::from_ptr`].
#[no_mangle]
pub unsafe extern "C" fn strstr(s: *const c_char, substr: *const c_char) -> *const c_char {
    let s = CStr::from_ptr(s).to_bytes();
    let substr = CStr::from_ptr(substr).to_bytes();

    for ss in s.windows(substr.len()) {
        if ss == substr {
            return ss.as_ptr().cast();
        }
    }

    ptr::null()
}

/// # Safety
///
/// Same as [`CStr::from_ptr`].
#[no_mangle]
pub unsafe extern "C" fn strtok(s: *mut c_char, delim: *const c_char) -> *const c_char {
    static mut HS: *mut c_char = ptr::null_mut();
    strtok_r(s, delim, &mut HS)
}

/// # Safety
///
/// Same as [`CStr::from_ptr`].
#[no_mangle]
pub unsafe extern "C" fn strtok_r(
    s: *mut c_char,
    delim: *const c_char,
    lasts: *mut *mut c_char,
) -> *mut c_char {
    let mut hs = s;
    if hs.is_null() {
        if (*lasts).is_null() {
            return ptr::null_mut();
        }
        hs = *lasts;
    }

    // Skip past any extra delimiter left over from previous call
    hs = hs.add(strspn(hs, delim));
    if *hs == 0 {
        *lasts = ptr::null_mut();
        return ptr::null_mut();
    }

    // Build token by injecting null byte into delimiter
    let token = hs;
    hs = strpbrk(token, delim) as *mut c_char;
    *lasts = if !hs.is_null() {
        hs.write(0);
        hs.add(1)
    } else {
        ptr::null_mut()
    };

    token
}

/// # Safety
///
/// Same as [`CStr::from_ptr`].
#[no_mangle]
pub unsafe extern "C" fn strrchr(s: *const c_char, ch: c_int) -> *const c_char {
    let haystack = CStr::from_ptr(s).to_bytes();
    let pos = memchr::memrchr(ch as u8, haystack);
    pos.map_or(ptr::null(), |pos| unsafe { s.add(pos) })
}

/// # Safety
///
/// The caller must ensure that `ptr` contains a valid byte slice with a length
/// of at least `count`.
#[no_mangle]
pub unsafe extern "C" fn memchr(ptr: *const c_void, ch: c_int, count: c_size_t) -> *const c_void {
    // SAFETY: The safety check is guaranteed by the caller.
    let haystack = unsafe { slice::from_raw_parts(ptr.cast(), count) };
    let pos = memchr::memchr(ch as u8, haystack);
    pos.map_or(ptr::null(), |pos| unsafe { ptr.add(pos) })
}

// Defined in `compiler_builtins`. TODO: implement this self with SIMD
// optimizations.
extern "C" {
    pub fn memcmp(lhs: *const c_void, rhs: *const c_void, count: c_size_t) -> c_int;

    pub fn memset(dest: *mut c_void, ch: c_int, count: c_size_t) -> *mut c_void;

    pub fn memcpy(dest: *mut c_void, src: *const c_void, count: c_size_t) -> *mut c_void;

    pub fn memmove(dest: *mut c_void, src: *const c_void, count: c_size_t) -> *mut c_void;
}

#[no_mangle]
pub extern "C" fn strerror(errnum: c_int) -> *const c_char {
    solvent::error::Error::desc_by_index(errnum).map_or(ptr::null(), |s| s.as_ptr().cast())
}

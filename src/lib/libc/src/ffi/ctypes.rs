#![allow(non_camel_case_types)]

pub use core::ffi::*;

#[no_mangle]
pub extern "C" fn isalnum(x: c_int) -> c_int {
    c_int::from(isalpha(x) != 0 || isdigit(x) != 0)
}

#[no_mangle]
pub extern "C" fn isalpha(x: c_int) -> c_int {
    c_int::from(isupper(x) != 0 || islower(x) != 0)
}

#[no_mangle]
pub extern "C" fn islower(x: c_int) -> c_int {
    c_int::from(matches!(x as u8, b'a'..=b'z'))
}

#[no_mangle]
pub extern "C" fn isupper(x: c_int) -> c_int {
    c_int::from(matches!(x as u8, b'A'..=b'Z'))
}

#[no_mangle]
pub extern "C" fn isdigit(x: c_int) -> c_int {
    c_int::from(matches!(x as u8, b'0'..=b'9'))
}

#[no_mangle]
pub extern "C" fn isxdigit(x: c_int) -> c_int {
    c_int::from(matches!(x as u8, b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F'))
}

#[no_mangle]
pub extern "C" fn iscntrl(x: c_int) -> c_int {
    c_int::from(matches!(x, 0x00..=0x1f | 0x7f))
}

#[no_mangle]
pub extern "C" fn isgraph(x: c_int) -> c_int {
    c_int::from(matches!(x, 0x21..=0x7e))
}

#[no_mangle]
pub extern "C" fn isspace(x: c_int) -> c_int {
    c_int::from(matches!(
        x as u8,
        b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c
    ))
}

#[no_mangle]
pub extern "C" fn isblank(x: c_int) -> c_int {
    c_int::from(matches!(x as u8, b' ' | b'\t'))
}

#[no_mangle]
pub extern "C" fn isprint(x: c_int) -> c_int {
    c_int::from((0x20..0x7f).contains(&x))
}

#[no_mangle]
pub extern "C" fn ispunct(x: c_int) -> c_int {
    c_int::from(matches!(x as u8,  b'!'..=b'/' | b':'..=b'@' | b'['..=b'`' | b'{'..=b'~'))
}

#[no_mangle]
pub extern "C" fn tolower(x: c_int) -> c_int {
    if isupper(x) != 0 {
        x | 32
    } else {
        x
    }
}

#[no_mangle]
pub extern "C" fn toupper(x: c_int) -> c_int {
    if islower(x) != 0 {
        x & !32
    } else {
        x
    }
}

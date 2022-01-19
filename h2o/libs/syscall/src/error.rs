use core::{fmt::Debug, ops::Range};

use crate::SerdeReg;

pub const ERRC_RANGE: Range<i32> = 1..35;
pub const CUSTOM_RANGE: Range<i32> = 1001..1005;

pub type Result<T = ()> = core::result::Result<T, Error>;

impl<T: SerdeReg> SerdeReg for Result<T> {
    #[inline]
    fn encode(self) -> usize {
        match self {
            Ok(t) => t.encode(),
            Err(e) => (-e.raw()) as usize,
        }
    }

    #[inline]
    fn decode(val: usize) -> Self {
        let err = -(val as i32);
        if ERRC_RANGE.contains(&err) || CUSTOM_RANGE.contains(&err) {
            Err(Error(err))
        } else {
            Ok(T::decode(val))
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct Error(i32);

impl Error {
    pub fn desc(&self) -> &'static str {
        if ERRC_RANGE.contains(&self.0) {
            ERRC_DESC[self.0 as usize]
        } else {
            CUSTOM_DESC[(self.0 - Self::CUSTOM_OFFSET) as usize]
        }
    }

    #[inline]
    pub fn raw(&self) -> i32 {
        self.0
    }

    #[inline]
    pub fn into_retval(self) -> usize {
        Err::<(), _>(self).encode()
    }
}

impl Debug for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Error: {}", self.desc())
    }
}

impl From<core::alloc::AllocError> for Error {
    #[inline]
    fn from(_: core::alloc::AllocError) -> Self {
        Error::ENOMEM
    }
}

impl From<core::alloc::LayoutError> for Error {
    #[inline]
    fn from(_: core::alloc::LayoutError) -> Self {
        Error::ENOMEM
    }
}

impl From<core::num::TryFromIntError> for Error {
    #[inline]
    fn from(_: core::num::TryFromIntError) -> Self {
        Error::EINVAL
    }
}

impl From<core::str::Utf8Error> for Error {
    #[inline]
    fn from(_: core::str::Utf8Error) -> Self {
        Error::EINVAL
    }
}

macro_rules! declare_error {
    ($e:ident, $v:literal, $desc:literal) => {
        #[doc = $desc]
        pub const $e: Error = Error($v);
    };
}

impl Error {
    declare_error!(INVALID, 0, "(Invalid value)");
    declare_error!(EPERM, 1, "Operation not permitted");
    declare_error!(ENOENT, 2, "No such file or directory");
    declare_error!(ESRCH, 3, "No such process");
    declare_error!(EINTR, 4, "Interrupted system call");
    declare_error!(EIO, 5, "I/O error");
    declare_error!(ENXIO, 6, "No such device or address");
    declare_error!(E2BIG, 7, "Argument list too long");
    declare_error!(ENOEXEC, 8, "Exec format error");
    declare_error!(EBADF, 9, "Bad file number");
    declare_error!(ECHILD, 10, "No child processes");
    declare_error!(EAGAIN, 11, "Try again");
    declare_error!(ENOMEM, 12, "Out of memory");
    declare_error!(EACCES, 13, "Permission denied");
    declare_error!(EFAULT, 14, "Bad address");
    declare_error!(ENOTBLK, 15, "Block device required");
    declare_error!(EBUSY, 16, "Device or resource busy");
    declare_error!(EEXIST, 17, "File exists");
    declare_error!(EXDEV, 18, "Cross-device link");
    declare_error!(ENODEV, 19, "No such device");
    declare_error!(ENOTDIR, 20, "Not a directory");
    declare_error!(EISDIR, 21, "Is a directory");
    declare_error!(EINVAL, 22, "Invalid argument");
    declare_error!(ENFILE, 23, "File table overflow");
    declare_error!(EMFILE, 24, "Too many open files");
    declare_error!(ENOTTY, 25, "Not a typewriter");
    declare_error!(ETXTBSY, 26, "Text file busy");
    declare_error!(EFBIG, 27, "File too large");
    declare_error!(ENOSPC, 28, "No space left on device");
    declare_error!(ESPIPE, 29, "Illegal seek");
    declare_error!(EROFS, 30, "Read-only file system");
    declare_error!(EMLINK, 31, "Too many links");
    declare_error!(EPIPE, 32, "Broken pipe");
    declare_error!(EDOM, 33, "Math argument out of domain of func");
    declare_error!(ERANGE, 34, "Range not available");

    const CUSTOM_OFFSET: i32 = CUSTOM_RANGE.start;
    declare_error!(EKILLED, 1001, "Task already killed");
    declare_error!(EBUFFER, 1002, "Buffer range exceeded");
    declare_error!(ETIME, 1003, "Timed out");
    declare_error!(EALIGN, 1004, "Pointer unaligned");
    declare_error!(ETYPE, 1005, "Object type mismatch");
    declare_error!(ESPRT, 1006, "Function not supported");
}

const ERRC_DESC: &[&str] = &[
    "OK",
    "Operation not permitted",
    "No such file or directory",
    "No such process",
    "Interrupted system call",
    "I/O error",
    "No such device or address",
    "Argument list too long",
    "Exec format error",
    "Bad file number",
    "No child processes",
    "Try again",
    "Out of memory",
    "Permission denied",
    "Bad address",
    "Block device required",
    "Device or resource busy",
    "File exists",
    "Cross-device link",
    "No such device",
    "Not a directory",
    "Is a directory",
    "Invalid argument",
    "File table overflow",
    "Too many open files",
    "Not a typewriter",
    "Text file busy",
    "File too large",
    "No space left on device",
    "Illegal seek",
    "Read-only file system",
    "Too many links",
    "Broken pipe",
    "Math argument out of domain of func",
    "Range not available",
];

const CUSTOM_DESC: &[&str] = &[
    "Task already killed",
    "Buffer range exceeded",
    "Timed out",
    "Pointer unaligned",
];

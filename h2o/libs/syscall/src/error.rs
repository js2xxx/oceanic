pub mod c_ty;

use core::{
    fmt::{Debug, Display},
    ops::Range,
};

use crate::SerdeReg;

pub const ERRC_RANGE: Range<i32> = 1..35;
pub const CUSTOM_RANGE: Range<i32> = 1001..1007;

pub type Result<T = ()> = core::result::Result<T, Error>;

impl<T: SerdeReg> SerdeReg for Result<T> {
    #[inline]
    fn encode(self) -> usize {
        match self {
            Ok(t) => t.encode(),
            Err(e) => e.raw() as usize,
        }
    }

    #[inline]
    fn decode(val: usize) -> Self {
        Error::try_decode(val).map_or_else(|| Ok(T::decode(val)), Err)
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct Error {
    raw: i32,
}

impl core::error::Error for Error {}

impl Error {
    fn try_decode(val: usize) -> Option<Self> {
        let err = -(val as i32);
        if ERRC_RANGE.contains(&err) || CUSTOM_RANGE.contains(&err) {
            Some(Error { raw: -err })
        } else {
            None
        }
    }

    pub fn desc(&self) -> &'static str {
        let index = -self.raw;
        if ERRC_RANGE.contains(&index) {
            ERRC_DESC[index as usize]
        } else {
            CUSTOM_DESC[(index - CUSTOM_OFFSET) as usize]
        }
    }

    pub fn name(&self) -> &'static str {
        let index = -self.raw;
        if ERRC_RANGE.contains(&index) {
            ERRC_NAME[index as usize]
        } else {
            CUSTOM_NAME[(index - CUSTOM_OFFSET) as usize]
        }
    }

    pub fn desc_by_index(errnum: i32) -> Option<&'static str> {
        let index = -errnum as usize;
        { ERRC_DESC.get(index) }
            .or_else(|| CUSTOM_DESC.get(index - CUSTOM_OFFSET as usize))
            .copied()
    }

    #[inline]
    pub fn raw(&self) -> i32 {
        self.raw
    }

    #[inline]
    pub fn into_retval(self) -> usize {
        Err::<(), _>(self).encode()
    }

    pub fn try_from_retval(retval: usize) -> Option<Self> {
        Self::try_decode(retval)
    }
}

impl Debug for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.desc())
    }
}

impl From<core::alloc::AllocError> for Error {
    #[inline]
    fn from(_: core::alloc::AllocError) -> Self {
        ENOMEM
    }
}

impl From<core::alloc::LayoutError> for Error {
    #[inline]
    fn from(_: core::alloc::LayoutError) -> Self {
        ENOMEM
    }
}

impl From<core::num::TryFromIntError> for Error {
    #[inline]
    fn from(_: core::num::TryFromIntError) -> Self {
        EINVAL
    }
}

impl From<core::str::Utf8Error> for Error {
    #[inline]
    fn from(_: core::str::Utf8Error) -> Self {
        EINVAL
    }
}

macro_rules! declare_errors {
    (($descs:ident, $names:ident, $num:expr) => {
        $(const $e:ident = Error { $v:literal, $desc:literal };)*
    }) => {
        $(
            #[doc = $desc]
            pub const $e: Error = Error { raw: -$v };
        )*

        const $descs: [&str; $num] = [$($desc),*];
        const $names: [&str; $num] = [$(stringify!($e)),*];
    };
}

declare_errors! {
    (ERRC_DESC, ERRC_NAME, (ERRC_RANGE.end - ERRC_RANGE.start + 1) as usize) => {
        const OK       = Error { 0, "Success" };
        const EPERM    = Error { 1, "Operation not permitted" };
        const ENOENT   = Error { 2, "No such file or directory" };
        const ESRCH    = Error { 3, "No such process" };
        const EINTR    = Error { 4, "Interrupted system call" };
        const EIO      = Error { 5, "I/O error" };
        const ENXIO    = Error { 6, "No such device or address" };
        const E2BIG    = Error { 7, "Argument list too long" };
        const ENOEXEC  = Error { 8, "Executable format error" };
        const EBADF    = Error { 9, "Bad file number" };
        const ECHILD   = Error { 10, "No child processes" };
        const EAGAIN   = Error { 11, "Try again" };
        const ENOMEM   = Error { 12, "Out of memory" };
        const EACCES   = Error { 13, "Permission denied" };
        const EFAULT   = Error { 14, "Bad address" };
        const ENOTBLK  = Error { 15, "Block device required" };
        const EBUSY    = Error { 16, "Device or resource busy" };
        const EEXIST   = Error { 17, "File exists" };
        const EXDEV    = Error { 18, "Cross-device link" };
        const ENODEV   = Error { 19, "No such device" };
        const ENOTDIR  = Error { 20, "Not a directory" };
        const EISDIR   = Error { 21, "Is a directory" };
        const EINVAL   = Error { 22, "Invalid argument" };
        const ENFILE   = Error { 23, "File table overflow" };
        const EMFILE   = Error { 24, "Too many open files" };
        const ENOTTY   = Error { 25, "Not a typewriter" };
        const ETXTBSY  = Error { 26, "Text file busy" };
        const EFBIG    = Error { 27, "File too large" };
        const ENOSPC   = Error { 28, "No space left on device" };
        const ESPIPE   = Error { 29, "Illegal seek" };
        const EROFS    = Error { 30, "Read-only file system" };
        const EMLINK   = Error { 31, "Too many links" };
        const EPIPE    = Error { 32, "Broken pipe" };
        const EDOM     = Error { 33, "Math argument out of domain of func" };
        const ERANGE   = Error { 34, "Range not available" };
    }
}

const CUSTOM_OFFSET: i32 = CUSTOM_RANGE.start;
declare_errors! {
    (CUSTOM_DESC, CUSTOM_NAME, (CUSTOM_RANGE.end - CUSTOM_RANGE.start) as usize) => {
        const EKILLED = Error { 1001, "Object already killed" };
        const EBUFFER = Error { 1002, "Buffer range exceeded" };
        const ETIME   = Error { 1003, "Timed out" };
        const EALIGN  = Error { 1004, "Pointer unaligned" };
        const ETYPE   = Error { 1005, "Object type mismatch" };
        const ESPRT   = Error { 1006, "Function not supported" };
    }
}

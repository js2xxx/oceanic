use core::{fmt::Debug, ops::Range};

pub const ERRC_RANGE: Range<i32> = 1..35;
pub const CUSTOM_RANGE: Range<i32> = 1000..1005;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct Error(pub i32);

impl Error {
    pub fn encode(res: Result<usize>) -> usize {
        res.map_err(|err| -err.0 as usize).into_ok_or_err()
    }

    pub fn decode(val: usize) -> Result<usize> {
        let errc = -(val as i32);
        (!ERRC_RANGE.contains(&errc) && !(CUSTOM_RANGE.contains(&errc)))
            .then_some(val)
            .ok_or(Error(errc))
    }

    pub fn desc(&self) -> &'static str {
        ERRC_DESC[self.0 as usize]
    }
}

impl Debug for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Error: {}", self.desc())
    }
}

/// Operation not permitted
pub const EPERM: i32 = 1;
/// No such file or directory
pub const ENOENT: i32 = 2;
/// No such process
pub const ESRCH: i32 = 3;
/// Interrupted system call
pub const EINTR: i32 = 4;
/// I/O error
pub const EIO: i32 = 5;
/// No such device or address
pub const ENXIO: i32 = 6;
/// Argument list too long
pub const E2BIG: i32 = 7;
/// Exec format error
pub const ENOEXEC: i32 = 8;
/// Bad file number
pub const EBADF: i32 = 9;
/// No child processes
pub const ECHILD: i32 = 10;
/// Try again
pub const EAGAIN: i32 = 11;
/// Out of memory
pub const ENOMEM: i32 = 12;
/// Permission denied
pub const EACCES: i32 = 13;
/// Bad address
pub const EFAULT: i32 = 14;
/// Block device required
pub const ENOTBLK: i32 = 15;
/// Device or resource busy
pub const EBUSY: i32 = 16;
/// File exists
pub const EEXIST: i32 = 17;
/// Cross-device link
pub const EXDEV: i32 = 18;
/// No such device
pub const ENODEV: i32 = 19;
/// Not a directory
pub const ENOTDIR: i32 = 20;
/// Is a directory
pub const EISDIR: i32 = 21;
/// Invalid argument
pub const EINVAL: i32 = 22;
/// File table overflow
pub const ENFILE: i32 = 23;
/// Too many open files
pub const EMFILE: i32 = 24;
/// Not a typewriter
pub const ENOTTY: i32 = 25;
/// Text file busy
pub const ETXTBSY: i32 = 26;
/// File too large
pub const EFBIG: i32 = 27;
/// No space left on device
pub const ENOSPC: i32 = 28;
/// Illegal seek
pub const ESPIPE: i32 = 29;
/// Read-only file system
pub const EROFS: i32 = 30;
/// Too many links
pub const EMLINK: i32 = 31;
/// Broken pipe
pub const EPIPE: i32 = 32;
/// Math argument out of domain of func
pub const EDOM: i32 = 33;
/// Math result not representable
pub const ERANGE: i32 = 34;

pub const EKILLED: i32 = 1001;
pub const EBUFFER: i32 = 1002;
pub const ETIME: i32 = 1003;

const ERRC_DESC: [&str; ERRC_RANGE.end as usize] = [
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
    "Math result not representable",
];

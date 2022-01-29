//! # Syscall module for the H2O
//!
//! ## Adding a syscall (`fn cast_init(k: *mut K) -> *const L`)
//!
//! Just create a private submodule `syscall` in a file and write the processing
//! code:
//!
//! ```rust,no_run
//! mod syscall {
//!       use sv_call::*;
//!       #[syscall]
//!       fn cast_init(k: *mut K) -> *const L {
//!             init(k);
//!             Ok(k.cast())
//!       }
//! }
//! ```
//!
//! And the `xtask` will generate the wrapper stub and the caller stub for you.

mod user_ptr;

use sv_call::*;

pub use self::user_ptr::*;

type SyscallWrapper = unsafe extern "C" fn(usize, usize, usize, usize, usize) -> usize;
static SYSCALL_TABLE: &[SyscallWrapper] =
    &include!(concat!(env!("CARGO_MANIFEST_DIR"), "/target/wrapper.rs"));

pub fn handler(num: usize, args: &[usize; 5]) -> usize {
    match SYSCALL_TABLE.get(num).copied() {
        Some(handler) => unsafe { handler(args[0], args[1], args[2], args[3], args[4]) },
        _ => Error::EINVAL.into_retval(),
    }
}

//! # Syscall module for the H2O
//!
//! ## Adding a syscall (`fn cast_init(k: *mut K) -> *const L`)
//!
//! 1. Add a new JSON file in the target directory of the kernel source root
//! with the following content as the prototype, or append it to an existing
//! file:
//!
//! ```json
//! {
//!     "name": "sv_cast_init",
//!     "returns": "*const L",
//!     "args": [
//!         {
//!             "name": "k",
//!             "ty": "*mut K"
//!         }
//!     ]
//! }
//! ```
//!
//! 2. Create a private submodule `syscall` in a source file and write the
//! processing code:
//!
//! ```rust,no_run
//! mod syscall {
//!       use sv_call::*;
//!       #[syscall]
//!       fn cast_init(k: *mut K) -> Result<*const L> {
//!             // init(k);
//!             Ok(k.cast())
//!       }
//! }
//! ```
//!
//! And the `xtask` will generate the wrapper stub and the caller stub for you.

mod user_ptr;

use sv_call::{call::Syscall, *};

pub use self::user_ptr::*;

type SyscallWrapper = unsafe extern "C" fn(usize, usize, usize, usize, usize) -> usize;
static SYSCALL_TABLE: &[SyscallWrapper] =
    &include!(concat!(env!("CARGO_MANIFEST_DIR"), "/target/wrapper.rs"));

pub fn handle(syscall: &mut Syscall) {
    let args = syscall.args;
    let result = match SYSCALL_TABLE.get(syscall.num).copied() {
        Some(handler) => unsafe { handler(args[0], args[1], args[2], args[3], args[4]) },
        _ => EINVAL.into_retval(),
    };
    syscall.result = result
}

/// An example of syscall
#[allow(clippy::module_inception)]
mod syscall {
    use sv_call::*;

    use crate::sched::SCHED;

    #[syscall]
    fn int_new(value: u64) -> Result<Handle> {
        SCHED.with_current(|cur| unsafe {
            cur.space().handles().insert_unchecked(
                value,
                Feature::SEND | Feature::SYNC | Feature::READ,
                None,
            )
        })
    }

    #[syscall]
    fn int_get(hdl: Handle) -> Result<u64> {
        SCHED.with_current(|cur| cur.space().handles().get::<u64>(hdl).map(|obj| ***obj))
    }
}

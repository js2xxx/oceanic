//! # Syscall module for the H2O
//!
//! ## Adding a syscall (`fn cast_init(k: *mut K) -> *const L`)
//!
//! 1.  Add a prototype definition to the module [`solvent::call`]:
//!
//! ```rust,no_run
//! solvent_gen::syscall_stub!(0 => pub(crate) fn cast_init(k: *mut K) -> *const L);
//! ```
//!
//! 2.  In the kernel, create a private submodule `syscall` in a file and write
//! the processing code:
//!
//! ```rust,no_run
//! mod syscall {
//!       use solvent::*;
//!       #[syscall]
//!       fn cast_init(k: *mut K) -> *const L {
//!             init(k);
//!             Ok(k.cast())
//!       }
//! }
//! ```
//!
//! 3.  Add a corresponding slot to the [`SYSCALL_TABLE`] in the position:
//!
//! ```rust,no_run
//! static SYSCALL_TABLE: &[Option<SyscallWrapper>] = &[
//!       ...,
//!       Some(syscall_wrapper!(cast_init))
//! ];
//! ```

mod user_ptr;

use solvent::*;
pub use user_ptr::*;

static SYSCALL_TABLE: &[Option<SyscallWrapper>] = &[
    Some(syscall_wrapper!(get_time)),
    Some(syscall_wrapper!(log)),
    Some(syscall_wrapper!(task_exit)),
    Some(syscall_wrapper!(task_exec)),
    Some(syscall_wrapper!(task_new)),
    Some(syscall_wrapper!(task_join)),
    Some(syscall_wrapper!(task_ctl)),
    Some(syscall_wrapper!(task_debug)),
    Some(syscall_wrapper!(task_sleep)),
    Some(syscall_wrapper!(phys_alloc)),
    Some(syscall_wrapper!(mem_new)),
    Some(syscall_wrapper!(mem_map)),
    Some(syscall_wrapper!(mem_reprot)),
    Some(syscall_wrapper!(mem_unmap)),
    None,
    None,
    Some(syscall_wrapper!(futex_wait)),
    Some(syscall_wrapper!(futex_wake)),
    Some(syscall_wrapper!(futex_reque)),
    Some(syscall_wrapper!(obj_clone)),
    Some(syscall_wrapper!(obj_drop)),
    None,
    None,
    Some(syscall_wrapper!(chan_new)),
    Some(syscall_wrapper!(chan_send)),
    Some(syscall_wrapper!(chan_recv)),
    Some(syscall_wrapper!(chan_csend)),
    Some(syscall_wrapper!(chan_crecv)),
    None,
    Some(syscall_wrapper!(event_new)),
    Some(syscall_wrapper!(event_wait)),
    Some(syscall_wrapper!(event_notify)),
    Some(syscall_wrapper!(event_endn)),
];

pub fn handler(arg: &Arguments) -> usize {
    match SYSCALL_TABLE.get(arg.fn_num).copied() {
        Some(Some(handler)) => unsafe {
            handler(
                arg.args[0],
                arg.args[1],
                arg.args[2],
                arg.args[3],
                arg.args[4],
            )
        },
        _ => Error::EINVAL.into_retval(),
    }
}

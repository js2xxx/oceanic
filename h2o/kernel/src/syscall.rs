//! # Syscall module for the H2O
//!
//! ## Adding a syscall (`fn cast_init(k: *mut K) -> *const L`)
//!
//! 1.  Add a prototype definition to the module [`solvent::call`]:
//!     ```rust, no_run
//!     solvent_gen::syscall_stub!(0 => pub(crate) fn cast_init(k: *mut K) -> *const L);
//!     ```
//! 2.  In the kernel, create a private submodule `syscall` in a file and write the processing
//!     code:
//!     ```rust,no_run
//!     mod syscall {
//!           use solvent::*;
//!           #[syscall]
//!           fn cast_init(k: *mut K) -> *const L {
//!                 init(k);
//!                 Ok(k.cast())
//!           }
//!     }
//!     ```
//! 3.  Add a corresponding slot to the [`SYSCALL_TABLE`] in the position:
//!     ```rust,no_run
//!     static SYSCALL_TABLE: &[Option<SyscallWrapper>] = &[
//!           ...,
//!           Some(syscall_wrapper!(cast_init))
//!     ];
//!     ```

use solvent::*;

static SYSCALL_TABLE: &[Option<SyscallWrapper>] = &[
      Some(syscall_wrapper!(get_time)),
      Some(syscall_wrapper!(exit)),
      Some(syscall_wrapper!(log)),
      Some(syscall_wrapper!(task_fn)),
];

pub fn handler(arg: &Arguments) -> solvent::Result<usize> {
      let h = if (0..SYSCALL_TABLE.len()).contains(&arg.fn_num) {
            SYSCALL_TABLE[arg.fn_num].ok_or(Error(EINVAL))?
      } else {
            return Err(Error(EINVAL));
      };

      let ret = unsafe {
            h(
                  arg.args[0],
                  arg.args[1],
                  arg.args[2],
                  arg.args[3],
                  arg.args[4],
            )
      };
      solvent::Error::decode(ret)
}

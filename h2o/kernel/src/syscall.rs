use solvent::*;

static SYSCALL_TABLE: &[SyscallWrapper] = &[syscall_wrapper!(get_time), syscall_wrapper!(log)];

pub fn handler(arg: &Arguments) -> solvent::Result<usize> {
      let h = if (0..SYSCALL_TABLE.len()).contains(&arg.fn_num) {
            SYSCALL_TABLE[arg.fn_num]
      } else {
            return Err(Error(EINVAL));
      };

      solvent::Error::decode(unsafe {
            h(
                  arg.args[0],
                  arg.args[1],
                  arg.args[2],
                  arg.args[3],
                  arg.args[4],
            )
      })
}

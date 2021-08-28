use solvent::*;

pub fn handler(arg: &Arguments) -> solvent::Result<usize> {
      match arg.fn_num {
            _ => return Err(solvent::Error(solvent::EACCES)),
      }
}

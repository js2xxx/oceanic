mod imp;

use std::{error::Error, path::Path};

pub fn gen_syscall(
    input: impl AsRef<Path>,
    wrapper_file: impl AsRef<Path>,
    call_file: impl AsRef<Path>,
) -> Result<(), Box<dyn Error>> {
    let funcs = crate::gen::imp::parse_dir(input)?;
    crate::gen::imp::gen_wrappers(&funcs, wrapper_file)?;
    crate::gen::imp::gen_rust_calls(&funcs, call_file)?;
    Ok(())
}

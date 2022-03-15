mod bootfs;
mod syscall;

use std::{error::Error, fs, io::BufWriter, path::Path};

pub fn gen_syscall(
    input: impl AsRef<Path>,
    wrapper_file: impl AsRef<Path>,
    call_file: impl AsRef<Path>,
    stub_file: impl AsRef<Path>,
) -> Result<(), Box<dyn Error>> {
    let funcs = crate::gen::syscall::parse_dir(input)?;
    syscall::gen_wrappers(&funcs, wrapper_file)?;
    syscall::gen_rust_calls(&funcs, call_file)?;
    syscall::gen_rust_stubs(&funcs, stub_file)?;
    Ok(())
}

pub fn gen_bootfs(output: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
    let data = bootfs::parse(crate::BOOTFS)?;
    let mut file = BufWriter::new(fs::File::create(output)?);
    ::bootfs::gen::generate(&data, &mut file)?;
    Ok(())
}

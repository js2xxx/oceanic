mod bootfs;
mod syscall;

use std::{error::Error, fs, io::BufWriter, path::Path};

use rand::{prelude::SliceRandom, thread_rng};

use self::syscall::Syscall;

pub fn gen_syscall(
    input: impl AsRef<Path>,
    wrapper_file: impl AsRef<Path>,
    call_file: impl AsRef<Path>,
    stub_file: impl AsRef<Path>,
    num_file: impl AsRef<Path>,
) -> Result<(), Box<dyn Error>> {
    let Syscall {
        mut types,
        mut funcs,
    } = crate::gen::syscall::parse_dir(input)?;

    types.shuffle(&mut thread_rng());

    funcs.shuffle(&mut thread_rng());
    let pos = funcs.iter().position(|func| &func.name == "sv_task_exit");
    if let Some(pos) = pos {
        funcs.swap(0, pos);
    }
    syscall::gen_wrappers(&funcs, wrapper_file)?;
    syscall::gen_rust_calls(&funcs, call_file)?;
    syscall::gen_rust_stubs(&funcs, stub_file)?;
    syscall::gen_rust_nums(&types, &funcs, num_file)?;
    Ok(())
}

pub fn gen_bootfs(output: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
    let data = bootfs::parse(crate::BOOTFS)?;
    let mut file = BufWriter::new(fs::File::create(output)?);
    ::bootfs::gen::generate(&data, &mut file)?;
    Ok(())
}

#[allow(clippy::module_inception)]
mod gen;
mod parse;

use std::{
    error::Error,
    io::{BufWriter, Write},
    path::Path,
};

use quote::ToTokens;

pub fn gen_syscall(
    input: impl AsRef<Path>,
    wrapper_file: impl AsRef<Path>,
    call_file: impl AsRef<Path>,
) -> Result<(), Box<dyn Error>> {
    let dir = std::fs::read_dir(input)?;
    let funcs = parse::parse_dir(dir)?;

    let wrapper_stubs = gen::wrapper_stubs(&funcs)?;
    let call_stubs = funcs
        .iter()
        .enumerate()
        .map(|(num, func)| gen::call_stub(num, func.clone()))
        .collect::<Result<Vec<_>, _>>()?;

    {
        println!("Writing to {:?}", wrapper_file.as_ref());
        let _ = std::fs::remove_file(wrapper_file.as_ref());
        let mut wrapper_file = std::fs::File::options()
            .create(true)
            .write(true)
            .truncate(true)
            .open(wrapper_file)?;
        let _ = wrapper_file.write(wrapper_stubs.to_token_stream().to_string().as_bytes())?;
    }

    {
        println!("Writing to {:?}", call_file.as_ref());
        let _ = std::fs::remove_file(call_file.as_ref());
        let mut call_file = std::fs::File::options()
            .create(true)
            .write(true)
            .truncate(true)
            .open(call_file)?;
        let mut writer = BufWriter::new(&mut call_file);
        for item in call_stubs.into_iter().flatten() {
            let _ = writer.write(item.to_token_stream().to_string().as_bytes())?;
        }
        writer.flush()?;
    }
    Ok(())
}

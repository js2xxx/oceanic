use std::{
    error::Error,
    fs,
    io::{BufWriter, Write},
    path::Path,
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SyscallArg {
    name: String,
    ty: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyscallFn {
    name: String,
    returns: String,
    args: Vec<SyscallArg>,
}

fn parse_file(file: impl AsRef<Path>) -> Result<Vec<SyscallFn>, Box<dyn Error>> {
    // println!("parsing file {:?}", file.as_ref());
    let file = fs::File::open(file)?;
    let ret: Vec<SyscallFn> = serde_json::from_reader(&file)?;
    Ok(ret)
}

pub fn parse_dir(dir: impl AsRef<Path>) -> Result<Vec<SyscallFn>, Box<dyn Error>> {
    fs::read_dir(dir)?
        .flatten()
        .try_fold(Vec::new(), |mut ret, ent| {
            let ty = ent.file_type()?;
            if ty.is_file() {
                ret.append(&mut parse_file(&ent.path())?);
            }
            Ok(ret)
        })
}

pub fn gen_wrappers(funcs: &[SyscallFn], output: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
    let mut output = BufWriter::new(fs::File::create(output)?);

    write!(output, "[")?;
    for func in funcs {
        let wrapper_name = format!("wrapper_{}", &func.name[3..]);
        write!(output, "{{ extern \"C\" {{ fn {}(", wrapper_name)?;
        write!(output, "a: usize, b: usize, c: usize, d: usize, e: usize")?;
        write!(output, ") -> usize; }} {} }},", wrapper_name)?;
    }
    write!(output, "]")?;

    output.flush()?;
    Ok(())
}

pub fn gen_rust_calls(funcs: &[SyscallFn], output: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
    let mut output = BufWriter::new(fs::File::create(output)?);

    for (i, func) in funcs.iter().enumerate() {
        write!(
            output,
            "#[no_mangle] pub unsafe extern \"C\" fn {}(",
            func.name
        )?;
        for arg in &func.args {
            write!(output, "{}: {}, ", arg.name, arg.ty)?;
        }
        let c_returns = match &*func.returns {
            "()" => "Status",
            "Handle" => "StatusOrHandle",
            _ => "StatusOrValue",
        };
        write!(output, ") -> {} {{ ", c_returns)?;
        write!(output, "let ret = unsafe {{ raw::syscall({}, ", i)?;
        for arg in &func.args {
            write!(output, "<{} as SerdeReg>::encode({}), ", arg.ty, arg.name)?;
        }
        for _ in 0..(5 - func.args.len()) {
            write!(output, "0, ")?;
        }
        write!(output, ") }}; SerdeReg::decode(ret) }} ")?;
    }

    output.flush()?;
    Ok(())
}

pub fn gen_rust_stubs(funcs: &[SyscallFn], output: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
    let mut output = BufWriter::new(fs::File::create(output)?);

    write!(output, "extern \"C\" {{")?;
    for func in funcs.iter() {
        write!(output, "pub fn {}(", func.name)?;
        for arg in &func.args {
            write!(output, "{}: {}, ", arg.name, arg.ty)?;
        }
        let c_returns = match &*func.returns {
            "()" => "Status",
            "Handle" => "StatusOrHandle",
            _ => "StatusOrValue",
        };
        write!(output, ") -> {}; ", c_returns)?;
    }
    write!(output, "}}")?;

    output.flush()?;
    Ok(())
}

use std::{
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
    pub name: String,
    returns: String,
    args: Vec<SyscallArg>,
    #[serde(default)]
    vdso_specific: bool,
    #[serde(default)]
    vdso_only: bool,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Syscall {
    pub types: Vec<String>,
    pub funcs: Vec<SyscallFn>,
}

impl Syscall {
    fn append(&mut self, other: &mut Self) {
        self.types.append(&mut other.types);
        self.funcs.append(&mut other.funcs);
    }
}

fn parse_file(file: impl AsRef<Path>) -> anyhow::Result<Syscall> {
    // println!("parsing file {:?}", file.as_ref());
    let file = fs::File::open(file)?;
    Ok(serde_json::from_reader(&file)?)
}

pub fn parse_dir(dir: impl AsRef<Path>) -> anyhow::Result<Syscall> {
    fs::read_dir(dir)?
        .flatten()
        .try_fold(Syscall::default(), |mut ret, ent| {
            let ty = ent.file_type()?;
            if ty.is_file() {
                ret.append(&mut parse_file(&ent.path())?);
            }
            Ok(ret)
        })
}

pub fn gen_wrappers(funcs: &[SyscallFn], output: impl AsRef<Path>) -> anyhow::Result<()> {
    let mut output = BufWriter::new(fs::File::create(output)?);

    write!(output, "[")?;
    for func in funcs {
        let wrapper_name = format!("wrapper_{}", &func.name[3..]);
        if !func.vdso_only {
            write!(output, "{{ extern \"C\" {{ fn {}(", wrapper_name)?;
            write!(output, "a: usize, b: usize, c: usize, d: usize, e: usize")?;
            write!(output, ") -> usize; }} {} }},", wrapper_name)?;
        } else {
            write!(output, "{{ extern \"C\" fn {}(", wrapper_name)?;
            write!(output, "_: usize, _: usize, _: usize, _: usize, _: usize")?;
            write!(output, ") -> usize {{ 0 }} {} }},", wrapper_name)?;
        }
    }
    write!(output, "]")?;

    output.flush()?;
    Ok(())
}

pub fn gen_rust_calls(funcs: &[SyscallFn], output: impl AsRef<Path>) -> anyhow::Result<()> {
    let mut output = BufWriter::new(fs::File::create(output)?);

    for (i, func) in funcs.iter().enumerate() {
        let c_returns = match &*func.returns {
            "()" => "Status",
            "Handle" => "StatusOrHandle",
            _ => "StatusOrValue",
        };
        if !func.vdso_only {
            if func.vdso_specific {
                write!(output, "#[cfg(not(feature = \"vdso\"))] ")?;
            }

            write!(
                output,
                "#[no_mangle] pub unsafe extern \"C\" fn {}(",
                func.name
            )?;
            for arg in &func.args {
                write!(output, "{}: {}, ", arg.name, arg.ty)?;
            }
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

        if !func.vdso_specific {
            let pack_name = format!("sv_pack_{}", &func.name[3..]);
            let unpack_name = format!("sv_unpack_{}", &func.name[3..]);

            write!(output, "#[no_mangle] pub extern \"C\" fn {}(", pack_name,)?;
            for arg in &func.args {
                write!(output, "{}: {}, ", arg.name, arg.ty)?;
            }
            write!(output, ") -> Syscall {{ ")?;
            write!(output, "raw::pack_syscall({}, ", i)?;
            for arg in &func.args {
                write!(output, "<{} as SerdeReg>::encode({}), ", arg.ty, arg.name)?;
            }
            for _ in 0..(5 - func.args.len()) {
                write!(output, "0, ")?;
            }
            write!(output, ") }} ")?;

            write!(output, "#[no_mangle] pub extern \"C\" fn {}(", unpack_name,)?;
            write!(output, "result: usize")?;
            write!(output, ") -> {} {{ ", c_returns)?;
            write!(output, "SerdeReg::decode(result) }} ")?;
        }
    }

    output.flush()?;
    Ok(())
}

pub fn gen_rust_stubs(funcs: &[SyscallFn], output: impl AsRef<Path>) -> anyhow::Result<()> {
    let mut output = BufWriter::new(fs::File::create(output)?);

    write!(output, "#[link(name = \"h2o\")] extern \"C\" {{")?;
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

        if !func.vdso_specific {
            let pack_name = format!("sv_pack_{}", &func.name[3..]);
            let unpack_name = format!("sv_unpack_{}", &func.name[3..]);

            write!(output, "pub fn {pack_name}(")?;
            for arg in &func.args {
                write!(output, "{}: {}, ", arg.name, arg.ty)?;
            }
            write!(output, ") -> Syscall; ")?;

            write!(output, "pub fn {unpack_name}(")?;
            write!(output, "result: usize")?;
            write!(output, ") -> {c_returns}; ")?;
        }
    }
    write!(output, "}}")?;

    output.flush()?;
    Ok(())
}

pub fn gen_rust_nums(
    types: &[String],
    funcs: &[SyscallFn],
    output: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let mut output = BufWriter::new(fs::File::create(output)?);

    for (i, func) in funcs.iter().enumerate() {
        write!(
            output,
            "pub const SV_{}: usize = {i}; ",
            func.name[3..].to_uppercase(),
        )?;
    }

    for (i, ty) in types.iter().enumerate() {
        write!(output, "pub const SV_{}: usize = {i}; ", ty.to_uppercase())?;
    }

    output.flush()?;
    Ok(())
}

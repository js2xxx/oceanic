#![feature(exit_status_error)]

use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn gen_bindings(sysroot: &Path, ffi: &Path) -> Result<(), Box<dyn Error>> {
    let sysroot = sysroot.join("usr/include");

    for ent in fs::read_dir(ffi)?.flatten() {
        println!("cargo:rerun-if-changed={:?}", ent.path());
        let file_name = ent.file_name();
        let name = file_name
            .to_str()
            .unwrap()
            .split('.')
            .find_map(|name| name.rsplit('_').next())
            .unwrap();
        let mut config = cbindgen::Config {
            sys_includes: vec!["stddef.h".into(), "stdint.h".into(), "h2o.h".into()],
            include_guard: Some(format!("_CO2_{}_H_", name.to_ascii_uppercase())),
            language: cbindgen::Language::C,
            style: cbindgen::Style::Tag,
            no_includes: true,
            cpp_compat: true,
            enumeration: cbindgen::EnumConfig {
                prefix_with_name: true,
                ..Default::default()
            },
            ..Default::default()
        };

        if name == "stdio" {
            config.sys_includes.push("stdarg.h".into());
        }

        let binding = cbindgen::Builder::new()
            .with_src(ent.path())
            .with_config(config)
            .generate()?;
        binding.write_to_file(sysroot.join(format!("{name}.h")));
    }
    Ok(())
}

fn build_crt0(root: &Path, sysroot: &Path) -> Result<(), Box<dyn Error>> {
    let target = sysroot.join("usr/lib/crt0.o");

    let src = root.join("crt/crt0.asm");
    println!("cargo:rerun-if-changed={src:?}");

    Command::new("nasm")
        .arg(src)
        .args(["-f", "elf64"])
        .arg(format!("-o{}", target.to_string_lossy()))
        .status()?
        .exit_ok()?;

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sysroot = root.join("../../../target/sysroot");
    let ffi = root.join("src/ffi");

    gen_bindings(&sysroot, &ffi)?;
    build_crt0(&root, &sysroot)?;
    Ok(())
}

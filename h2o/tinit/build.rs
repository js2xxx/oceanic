#![feature(exit_status_error)]

use std::{env, error::Error, path::Path};

#[cfg(target_arch = "x86_64")]
fn asm_build(input: &str, output: &str, flags: &[&str]) -> Result<(), Box<dyn Error>> {
    use std::process::Command;

    println!("cargo:rerun-if-changed={}", input);
    let mut cmd = Command::new("nasm");
    cmd.args(&[input, "-o", output])
        .args(flags)
        .status()?
        .exit_ok()?;

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    if cfg!(target_arch = "x86_64") {
        let target_dir = env::var("OUT_DIR")?;

        for file in Path::new("entry/x86_64").read_dir()?.flatten() {
            let mut dst_name = file.file_name().to_string_lossy().to_string();
            dst_name += ".o";

            let src_path = file.path();
            let dst_path = format!("{}/{}", target_dir, dst_name);

            asm_build(src_path.to_str().unwrap(), &dst_path, &["-f", "elf64"])?;
            println!("cargo:rustc-link-arg={}", dst_path);
            println!("cargo:rerun-if-changed={}", src_path.to_str().unwrap());
        }

        println!(
            "cargo:rustc-link-arg=-T{}/tinit.ld",
            env::var("CARGO_MANIFEST_DIR")?
        );
        println!(
            "cargo:rerun-if-changed={}/tinit.ld",
            env::var("CARGO_MANIFEST_DIR")?
        );
    }
    Ok(())
}

#![feature(exit_status_error)]

use std::{env, error::Error};

fn asm_build(input: &str, output: &str, flags: &[&str]) -> Result<(), Box<dyn Error>> {
    use std::process::Command;

    println!("cargo:rerun-if-changed={input}");
    let mut cmd = Command::new("nasm");
    cmd.args([input, "-o", output])
        .args(flags)
        .status()?
        .exit_ok()?;

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let target_dir = env::var("OUT_DIR")?;
    {
        let tram_src = "src/cpu/x86_64/apic/tram.asm";
        let tram_dst = format!("{target_dir}/tram");
        asm_build(tram_src, &tram_dst, &[])?;
    }

    println!(
        "cargo:rustc-link-arg=-T{}/h2o.ld",
        env::var("CARGO_MANIFEST_DIR")?
    );
    println!(
        "cargo:rerun-if-changed={}/h2o.ld",
        env::var("CARGO_MANIFEST_DIR")?
    );

    Ok(())
}

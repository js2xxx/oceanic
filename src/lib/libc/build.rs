// #![feature(exit_status_error)]
use std::error::Error;
use std::fs;

const CRT0: &str = "crt/crt0.rs";
const CRTI: &str = "crt/crti.asm";
const CRTN: &str = "crt/crtn.asm";

// fn asm_build(
//     input: impl AsRef<Path>,
//     output: impl AsRef<Path>,
//     flags: &[&str],
// ) -> Result<(), Box<dyn Error>> {
//     use std::process::Command;

//     println!(
//         "cargo:rerun-if-changed={}",
//         input.as_ref().to_str().unwrap()
//     );
//     let mut cmd = Command::new("nasm");
//     cmd.arg(input.as_ref())
//         .arg("-o")
//         .arg(output.as_ref())
//         .args(flags)
//         .status()?
//         .exit_ok()?;

//     Ok(())
// }



fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed={}", CRT0);
    println!("cargo:rerun-if-changed={}", CRTI);
    println!("cargo:rerun-if-changed={}", CRTN);

    // let root = env!("CARGO_MANIFEST_DIR");
    Ok(())
}

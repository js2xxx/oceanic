#![feature(exit_status_error)]

use std::{
    env,
    error::Error,
    path::{Path, PathBuf},
};

const ACPICA_H: &str = "../localdep/acpica.h";
const ACPICA_PATH: &str = "../localdep/acpica";
const C_TARGET: &str = "--target=x86_64-unknown-linux-gnu";
const C_FLAGS: &[&str] = &[
    C_TARGET,
    "-Wno-null-pointer-arithmetic",
    "-Wno-unused-parameter",
    "-Wno-address",
    "-Wno-sign-compare",
    "-Wno-pointer-to-int-cast",
    "-Wno-unused-value",
    "-Wno-unknown-pragmas",
    "-Wno-implicit-function-declaration",
    "-Wno-write-strings",
    "-fno-exceptions",
    "-fno-stack-protector",
    "-fno-builtin",
    "-std=c11",
    "-g",
    "-mcmodel=large",
];
const TARGET_DIR: &str = "../../build/h2o";

fn acpica_build() -> Result<(), Box<dyn Error>> {
    let cd = env::current_dir()?;
    let path = cd.join(TARGET_DIR).into_os_string().into_string().unwrap();
    let mut build = cc::Build::new();
    build
        .compiler("clang")
        .include(Path::new(ACPICA_PATH).join("include"))
        .files(
            Path::new(ACPICA_PATH)
                .read_dir()
                .unwrap()
                .flatten()
                .filter(|e| e.file_name() != "include")
                .map(|e| e.path().read_dir().map(|iter| iter.flatten()))
                .flatten()
                .flatten()
                .map(|e| e.path()),
        );
    for flag in C_FLAGS {
        build.flag(flag);
    }
    build.compile("acpica");

    println!("cargo:rerun-if-changed={}/localdep/acpica", path);
    println!("cargo:rustc-link-search=native={}/localdep/acpica", path);
    println!("cargo:rustc-link-lib=static=acpica");

    Ok(())
}

fn acpica_bindgen() -> Result<(), Box<dyn Error>> {
    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed={}", ACPICA_H);

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header(ACPICA_H)
        .clang_arg(C_TARGET)
        .use_core().ctypes_prefix("cty")
        .blocklist_function("AcpiOs.*")
        .layout_tests(false)
        .prepend_enum_name(false)
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR")?);
    bindings.write_to_file(out_path.join("acpica.rs"))?;

    Ok(())
}

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
    acpica_build()?;
    acpica_bindgen()?;

    if cfg!(target_arch = "x86_64") {
        let target_dir = env::var("OUT_DIR")?;
        {
            let tram_src = "src/cpu/x86_64/apic/tram.asm";
            let tram_dst = format!("{}/tram", target_dir);
            asm_build(tram_src, &tram_dst, &[])?;
        }

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
            "cargo:rustc-link-arg=-T{}/h2o.ld",
            env::var("CARGO_MANIFEST_DIR")?
        );
        println!(
            "cargo:rerun-if-changed={}/h2o.ld",
            env::var("CARGO_MANIFEST_DIR")?
        );
    }

    Ok(())
}

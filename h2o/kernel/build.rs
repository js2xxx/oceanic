use std::{env, path::PathBuf};

static ACPICA_H: &str = "../localdep/acpica.h";
static TARGET_DIR: &str = "../../build/h2o";

fn acpica_build() {
    let cd = env::current_dir().unwrap();
    let path = cd.join(TARGET_DIR).into_os_string().into_string().unwrap();
    println!(
        "cargo:rerun-if-changed={}/localdep/acpica/libacpica.a",
        path
    );
    println!("cargo:rustc-link-search=native={}/localdep/acpica", path);
    println!("cargo:rustc-link-lib=static=acpica");
}

fn acpica_bindgen() {
    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed={}", ACPICA_H);

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header(ACPICA_H)
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
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("acpica.rs"))
        .expect("Couldn't write bindings!");
}

#[cfg(target_arch = "x86_64")]
fn tram_build() {
    use std::process::Command;

    let target_dir = env::var("OUT_DIR").unwrap();
    let file = "src/cpu/x86_64/apic/tram.asm";

    println!("cargo:rerun-if-changed={}", file);
    let cmd = Command::new("nasm")
        .args(&[file, "-o", format!("{}/tram", target_dir).as_str()])
        .status()
        .expect("Failed to build the compiling command");

    assert!(cmd.success(), "Failed to compile `tram.asm`");
}

fn main() {
    acpica_build();
    acpica_bindgen();

    if cfg!(target_arch = "x86_64") {
        tram_build();
    }
}

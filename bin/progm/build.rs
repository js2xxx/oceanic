fn main() {
    println!("cargo:rustc-link-arg=--dynamic-linker=lib/ld-oceanic.so");
    println!("cargo:rustc-link-arg=-L{}/../../target", env!("CARGO_MANIFEST_DIR"));
    println!("cargo:rustc-link-arg=-lh2o");
}

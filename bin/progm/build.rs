fn main() {
    println!("cargo:rustc-link-arg=--dynamic-linker=lib/ld-oceanic.so");
}

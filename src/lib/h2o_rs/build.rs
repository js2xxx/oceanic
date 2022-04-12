fn main() {
    println!(
        "cargo:rustc-link-search={}/../../../target/sysroot/usr/lib",
        env!("CARGO_MANIFEST_DIR")
    );
}

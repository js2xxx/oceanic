fn main() {
    println!(
        "cargo:rustc-link-search={}/../../../target",
        env!("CARGO_MANIFEST_DIR")
    );
}

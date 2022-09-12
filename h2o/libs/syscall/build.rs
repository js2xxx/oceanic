fn main() {
    #[cfg(all(feature = "call", feature = "vdso"))]
    {
        std::thread::spawn(|| {
            let config = cbindgen::Config::from_file("cbindgen.toml").unwrap();
            println!("cargo:rerun-if-changed=cbindgen.toml");

            let src_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let bindings = cbindgen::Builder::new()
                .with_config(config)
                .with_crate(".")
                .generate()
                .unwrap();

            let c_target_path = src_dir.join("../../../target/sysroot/usr/include/h2o.h");
            bindings.write_to_file(c_target_path);
        });
    }
}

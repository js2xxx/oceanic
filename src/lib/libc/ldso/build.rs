use std::{error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sysroot = root.join("../../../../target/sysroot");

    let src = root.join("src/ffi.rs");
    let sysroot = sysroot.join("usr/include");

    let config = cbindgen::Config {
        sys_includes: vec!["stddef.h".into(), "h2o.h".into()],
        include_guard: Some("_CO2_DLFCN_H_".to_string()),
        language: cbindgen::Language::C,
        style: cbindgen::Style::Tag,
        no_includes: true,
        cpp_compat: true,
        enumeration: cbindgen::EnumConfig {
            prefix_with_name: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let binding = cbindgen::Builder::new()
        .with_config(config)
        .with_src(src)
        .generate()?;
    binding.write_to_file(sysroot.join("dlfcn.h"));

    Ok(())
}

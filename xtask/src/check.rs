use std::{
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{H2O_BOOT, H2O_KERNEL, H2O_TINIT};

pub(crate) fn check() -> Result<(), Box<dyn Error>> {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let src_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(1)
        .unwrap();

    let check = |dir: PathBuf| -> Result<(), Box<dyn Error>> {
        Command::new(&cargo)
            .current_dir(dir)
            .args(["clippy", "--message-format=json-diagnostic-short"])
            .status()?
            .exit_ok()
            .map_err(Into::into)
    };

    check(src_root.join(H2O_BOOT))?;

    check(src_root.join(H2O_KERNEL))?;

    check(src_root.join(H2O_TINIT))?;

    for ent in fs::read_dir(src_root.join("lib"))?.flatten() {
        check(ent.path())?;
    }
    check(src_root.join("lib/libc/ldso"))?;

    Ok(())
}

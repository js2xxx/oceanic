use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{H2O_BOOT, H2O_KERNEL, H2O_TINIT, OC_BIN, OC_DRV, OC_LIB};

pub(crate) fn check() -> anyhow::Result<()> {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let src_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(1)
        .unwrap();

    let check = |dir: PathBuf| -> anyhow::Result<()> {
        Command::new(&cargo)
            .current_dir(dir)
            .args(["clippy", "--message-format=json-diagnostic-short"])
            .status()?
            .exit_ok()
            .map_err(Into::into)
    };

    let check_all = |dir: PathBuf| -> anyhow::Result<()> {
        for ent in fs::read_dir(dir)?.flatten() {
            let ty = ent.file_type()?;
            let name = ent.file_name();
            if ty.is_dir() && name != ".cargo" {
                check(ent.path())?;
            }
        }
        Ok(())
    };

    check(src_root.join(H2O_BOOT))?;
    check(src_root.join(H2O_KERNEL))?;
    check(src_root.join(H2O_TINIT))?;

    check_all(src_root.join(OC_BIN))?;
    check_all(src_root.join(OC_DRV))?;
    check_all(src_root.join(OC_LIB))?;
    check(src_root.join(OC_LIB).join("libc/ldso"))?;

    Ok(())
}

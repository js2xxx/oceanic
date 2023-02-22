use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::Context;
use clap::Parser;

use crate::{H2O_BOOT, H2O_KERNEL, H2O_TINIT, OC_BIN, OC_DRV, OC_LIB};

#[derive(Debug, Parser)]
pub struct Check {
    #[arg(long, short)]
    json: bool,
}

impl Check {
    pub fn run(self) -> anyhow::Result<()> {
        let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

        let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let target_root = env::var("CARGO_TARGET_DIR")
            .unwrap_or_else(|_| src_root.join("target").to_string_lossy().to_string());

        crate::dist::create_dir_all(&target_root, src_root)?;

        // Generate syscall stubs
        crate::gen::gen_syscall(
            src_root.join(H2O_KERNEL).join("syscall"),
            src_root.join(H2O_KERNEL).join("target/wrapper.rs"),
            src_root.join("h2o/libs/syscall/target/call.rs"),
            src_root.join("h2o/libs/syscall/target/stub.rs"),
            src_root.join("h2o/libs/syscall/target/num.rs"),
        )
        .context("failed to generate syscalls")?;

        let check = |dir: PathBuf| -> anyhow::Result<()> {
            let mut cmd = Command::new(&cargo);
            let cmd = cmd.current_dir(dir).arg("clippy");
            if self.json {
                cmd.arg("--message-format=json-diagnostic-short");
            }
            cmd.args(["--", "-Dclippy::all", "-Dwarnings"]);
            cmd.status()?.exit_ok().map_err(Into::into)
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
}

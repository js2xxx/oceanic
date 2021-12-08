use std::{env, error::Error, fs, path::Path, process::Command};

use structopt::StructOpt;

const H2O_BOOT: &str = "h2o/boot";
const H2O_KERNEL: &str = "h2o/kernel";
const H2O_TINIT: &str = "h2o/tinit";

#[derive(Debug, StructOpt)]
pub enum Type {
    Iso,
}

#[derive(Debug, StructOpt)]
pub struct Dist {
    #[structopt(subcommand)]
    ty: Type,
    #[structopt(long = "--release", parse(from_flag))]
    release: bool,
}

impl Dist {
    pub fn build(self) -> Result<(), Box<dyn Error>> {
        let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let src_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(1)
            .unwrap();
        let target_dir = env::var("CARGO_TARGET_DIR")
            .unwrap_or_else(|_| src_root.join("target").to_string_lossy().to_string());

        {
            // Build h2o_boot
            log::info!("Building h2o_boot");

            let mut cmd = Command::new(&cargo);
            let cmd = cmd.current_dir(src_root.join(H2O_BOOT)).arg("build");
            if self.release {
                cmd.arg("--release");
            }
            cmd.status()?.exit_ok()?;

            // Copy the binary to target.
            let src = if self.release {
                Path::new(&target_dir).join("x86_64-unknown-uefi/release")
            } else {
                Path::new(&target_dir).join("x86_64-unknown-uefi/debug")
            }
            .join("h2o_boot.efi");
            fs::copy(src, Path::new(&target_dir).join("BootX64.efi"))?;
        }

        // Build h2o_kernel
        {
            log::info!("Building h2o_kernel");

            let mut cmd = Command::new(&cargo);
            let cmd = cmd.current_dir(src_root.join(H2O_KERNEL)).arg("build");
            if self.release {
                cmd.arg("--release");
            }
            cmd.status()?.exit_ok()?;

            // Copy the binary to target.
            let src = if self.release {
                Path::new(&target_dir).join("x86_64-h2o-kernel/release")
            } else {
                Path::new(&target_dir).join("x86_64-h2o-kernel/debug")
            }
            .join("h2o");
            fs::copy(src, Path::new(&target_dir).join("KERNEL"))?;
        }

        // Build h2o_tinit
        {
            log::info!("Building h2o_tinit");

            let mut cmd = Command::new(&cargo);
            let cmd = cmd.current_dir(src_root.join(H2O_TINIT)).arg("build");
            if self.release {
                cmd.arg("--release");
            }
            cmd.status()?.exit_ok()?;

            // Copy the binary to target.
            let src = if self.release {
                Path::new(&target_dir).join("x86_64-unknown-h2o/release")
            } else {
                Path::new(&target_dir).join("x86_64-unknown-h2o/debug")
            }
            .join("tinit");
            fs::copy(src, Path::new(&target_dir).join("TINIT"))?;
        }

        // Generate debug symbols
        log::info!("Generating debug symbols");
        Command::new("sh")
            .current_dir(src_root)
            .arg("scripts/gendbg.sh")
            .status()?
            .exit_ok()?;

        match &self.ty {
            Type::Iso => {
                // Generate img
                log::info!("Generating a hard disk image file");
                Command::new("sh")
                    .current_dir(src_root)
                    .arg("scripts/genimg.sh")
                    .status()?
                    .exit_ok()?;
            }
        }
        Ok(())
    }
}

use std::{env, error::Error, fs, path::Path, process::Command};

use structopt::StructOpt;

use crate::{H2O_BOOT, H2O_KERNEL, H2O_TINIT};

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
            println!("Building h2o_boot");

            let mut cmd = Command::new(&cargo);
            let cmd = cmd.current_dir(src_root.join(H2O_BOOT)).arg("build");
            if self.release {
                cmd.arg("--release");
            }
            cmd.status()?.exit_ok()?;

            // Copy the binary to target.
            let bin_dir = if self.release {
                Path::new(&target_dir).join("x86_64-unknown-uefi/release")
            } else {
                Path::new(&target_dir).join("x86_64-unknown-uefi/debug")
            };
            fs::copy(
                bin_dir.join("h2o_boot.efi"),
                Path::new(&target_dir).join("BootX64.efi"),
            )?;
        }

        // Build h2o_kernel
        {
            println!("Building h2o_kernel");

            let mut cmd = Command::new(&cargo);
            let cmd = cmd.current_dir(src_root.join(H2O_KERNEL)).arg("build");
            if self.release {
                cmd.arg("--release");
            }
            cmd.status()?.exit_ok()?;

            // Copy the binary to target.
            let bin_dir = if self.release {
                Path::new(&target_dir).join("x86_64-h2o-kernel/release")
            } else {
                Path::new(&target_dir).join("x86_64-h2o-kernel/debug")
            };
            fs::copy(bin_dir.join("h2o"), Path::new(&target_dir).join("KERNEL"))?;
        }

        // Build h2o_tinit
        {
            println!("Building h2o_tinit");

            let mut cmd = Command::new(&cargo);
            let cmd = cmd.current_dir(src_root.join(H2O_TINIT)).arg("build");
            if self.release {
                cmd.arg("--release");
            }
            cmd.status()?.exit_ok()?;

            // Copy the binary to target.
            let bin_dir = if self.release {
                Path::new(&target_dir).join("x86_64-unknown-h2o/release")
            } else {
                Path::new(&target_dir).join("x86_64-unknown-h2o/debug")
            };
            fs::copy(bin_dir.join("tinit"), Path::new(&target_dir).join("TINIT"))?;
        }

        // Generate debug symbols
        println!("Generating debug symbols");
        Command::new("sh")
            .current_dir(src_root)
            .arg("scripts/gendbg.sh")
            .status()?
            .exit_ok()?;

        match &self.ty {
            Type::Iso => {
                // Generate img
                println!("Generating a hard disk image file");
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

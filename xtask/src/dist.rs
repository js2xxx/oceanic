use std::{env, error::Error, fs, path::Path, process::Command};

use structopt::StructOpt;

use crate::{H2O_BOOT, H2O_KERNEL, H2O_SYSCALL, H2O_TINIT};

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

        // Generate syscall stubs
        crate::gen::gen_syscall(
            src_root.join(H2O_KERNEL).join("syscall"),
            src_root.join(H2O_KERNEL).join("target/wrapper.rs"),
            src_root.join("h2o/libs/syscall/target/call.rs"),
            src_root.join("h2o/libs/syscall/target/stub.rs"),
        )?;

        // Build h2o_boot
        {
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

        // Build the VDSO
        {
            let target_triple = src_root.join(".cargo/x86_64-pc-oceanic.json");
            let cd = src_root.join(H2O_SYSCALL);
            let ldscript = cd.join("syscall.ld");

            fs::copy(cd.join("target/rxx.rs.in"), cd.join("target/rxx.rs"))?;

            let mut cmd = Command::new(&cargo);
            let cmd = cmd.current_dir(&cd).arg("rustc").args([
                "--crate-type=cdylib",
                &format!("--target={}", target_triple.to_string_lossy()),
                "-Zunstable-options",
                "-Zbuild-std=core,compiler_builtins,alloc,panic_abort",
                "-Zbuild-std-features=compiler-builtins-mem",
                "--release", /* VDSO can always be the release version and discard the debug
                              * symbols. */
                "--no-default-features",
                "--features",
                "call",
            ]);
            cmd.args([
                "--",
                &format!("-Clink-arg=-T{}", ldscript.to_string_lossy()),
            ])
            .status()?
            .exit_ok()?;

            // Copy the binary to target.
            let bin_dir = Path::new(&target_dir).join("x86_64-pc-oceanic/release");
            fs::copy(
                bin_dir.join("libsv_call.so"),
                src_root.join(H2O_KERNEL).join("target/vdso"),
            )?;

            fs::File::options()
                .create(true)
                .write(true)
                .truncate(true)
                .open(cd.join("target/rxx.rs"))?;
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
                Path::new(&target_dir).join("x86_64-h2o-tinit/release")
            } else {
                Path::new(&target_dir).join("x86_64-h2o-tinit/debug")
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

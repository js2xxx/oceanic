use std::{env, error::Error, fs, path::Path, process::Command};

use structopt::StructOpt;

use crate::{BOOTFS, H2O_BOOT, H2O_KERNEL, H2O_SYSCALL, H2O_TINIT};

#[derive(Debug, StructOpt)]
pub enum Type {
    Img,
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
        self.build_impl(
            &cargo,
            "h2o_boot.efi",
            "BootX64.efi",
            src_root.join(H2O_BOOT),
            &target_dir,
            "x86_64-unknown-uefi",
        )?;

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

            fs::File::create(cd.join("target/rxx.rs"))?;
        }

        // Build h2o_kernel
        self.build_impl(
            &cargo,
            "h2o",
            "KERNEL",
            src_root.join(H2O_KERNEL),
            &target_dir,
            "x86_64-h2o-kernel",
        )?;

        // Build h2o_tinit
        self.build_impl(
            &cargo,
            "tinit",
            "TINIT",
            src_root.join(H2O_TINIT),
            &target_dir,
            "x86_64-h2o-tinit",
        )?;

        crate::gen::gen_bootfs(Path::new(BOOTFS).join("../BOOT.fs"))?;

        // Generate debug symbols
        println!("Generating debug symbols");
        Command::new("sh")
            .current_dir(src_root)
            .arg("scripts/gendbg.sh")
            .status()?
            .exit_ok()?;

        match &self.ty {
            Type::Img => {
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

    fn build_impl(
        &self,
        cargo: &str,
        src_name: &str,
        dst_name: &str,
        src_dir: impl AsRef<Path>,
        target_dir: &str,
        target_triple: &str,
    ) -> Result<(), Box<dyn Error>> {
        println!("Building {}", dst_name);

        let mut cmd = Command::new(cargo);
        let cmd = cmd.current_dir(src_dir).arg("build");
        if self.release {
            cmd.arg("--release");
        }
        cmd.status()?.exit_ok()?;
        let bin_dir = if self.release {
            Path::new(target_dir).join(target_triple).join("release")
        } else {
            Path::new(target_dir).join(target_triple).join("debug")
        };
        fs::copy(
            bin_dir.join(src_name),
            Path::new(&target_dir).join(dst_name),
        )?;
        Ok(())
    }
}

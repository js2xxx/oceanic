use std::{
    collections::HashSet,
    env,
    error::Error,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use structopt::StructOpt;

use crate::{BOOTFS, DEBUG_DIR, H2O_BOOT, H2O_KERNEL, H2O_SYSCALL, H2O_TINIT, OC_BIN, OC_LIB};

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
    fn profile(&self) -> &'static str {
        if self.release {
            "release"
        } else {
            "debug"
        }
    }

    pub fn build(self) -> Result<(), Box<dyn Error>> {
        let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let target_root = env::var("CARGO_TARGET_DIR")
            .unwrap_or_else(|_| src_root.join("target").to_string_lossy().to_string());

        fs::create_dir_all(PathBuf::from(&target_root).join("bootfs/lib"))?;
        fs::create_dir_all(PathBuf::from(&target_root).join("bootfs/bin"))?;
        fs::create_dir_all(PathBuf::from(&target_root).join("sysroot/usr/include"))?;
        fs::create_dir_all(PathBuf::from(&target_root).join("sysroot/usr/lib"))?;

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
            Path::new(&target_root).join("x86_64-unknown-uefi"),
            &target_root,
        )?;

        // Build the VDSO
        {
            let target_triple = src_root.join(".cargo/x86_64-pc-oceanic.json");
            let cd = src_root.join(H2O_SYSCALL);
            let ldscript = cd.join("syscall.ld");

            println!("Building VDSO");

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
                "--features",
                "vdso",
            ]);
            cmd.args([
                "--",
                &format!("-Clink-arg=-T{}", ldscript.to_string_lossy()),
            ])
            .status()?
            .exit_ok()?;

            // Copy the binary to target.
            let bin_dir = Path::new(&target_root).join("x86_64-pc-oceanic/release");
            let path = src_root.join(H2O_KERNEL).join("target/vdso");
            fs::copy(bin_dir.join("libsv_call.so"), &path)?;
            Command::new("llvm-ifs")
                .arg("--input-format=ELF")
                .arg(format!(
                    "--output-elf={}/sysroot/usr/lib/libh2o.so",
                    target_root
                ))
                .arg(&path)
                .status()?
                .exit_ok()?;

            let out = Command::new("llvm-objdump")
                .arg("--syms")
                .arg(&path)
                .output()?
                .stdout;
            let s = String::from_utf8_lossy(&out);
            let (constants_offset, _) = s
                .split('\n')
                .find(|s| s.ends_with("CONSTANTS"))
                .and_then(|s| s.split_once(' '))
                .expect("Failed to get CONSTANTS");

            fs::write(
                src_root.join(H2O_KERNEL).join("target/constant_offset.rs"),
                format!("0x{}", constants_offset),
            )?;

            self.gen_debug("vdso", src_root.join(H2O_KERNEL).join("target"), DEBUG_DIR)?;
        }

        // Build h2o_kernel
        self.build_impl(
            &cargo,
            "h2o",
            "KERNEL",
            src_root.join(H2O_KERNEL),
            Path::new(&target_root).join("x86_64-h2o-kernel"),
            &target_root,
        )?;

        // Build h2o_tinit
        self.build_impl(
            &cargo,
            "tinit",
            "TINIT",
            src_root.join(H2O_TINIT),
            Path::new(&target_root).join("x86_64-h2o-tinit"),
            &target_root,
        )?;

        self.build_lib(&cargo, src_root, &target_root)?;
        self.build_bin(&cargo, src_root, &target_root)?;

        crate::gen::gen_bootfs(Path::new(BOOTFS).join("../BOOT.fs"))?;

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

    fn build_lib(
        &self,
        cargo: &str,
        src_root: impl AsRef<Path>,
        target_root: &str,
    ) -> Result<(), Box<dyn Error>> {
        let src_root = src_root.as_ref().join(OC_LIB);
        let bin_dir = Path::new(target_root).join("x86_64-pc-oceanic");
        let dst_root = Path::new(target_root).join("bootfs/lib");

        self.build_impl(
            cargo,
            "libldso.so",
            "ld-oceanic.so",
            src_root.join("libc/ldso"),
            &bin_dir,
            &dst_root,
        )?;

        Command::new("llvm-ifs")
            .arg("--input-format=ELF")
            .arg(format!(
                "--output-elf={}",
                Path::new(target_root)
                    .join("sysroot/usr/lib/libldso.so")
                    .to_string_lossy()
            ))
            .arg(bin_dir.join(self.profile()).join("libldso.so"))
            .status()?
            .exit_ok()?;

        self.build_impl(
            cargo,
            "libco2.so",
            "libco2.so",
            src_root.join("libc"),
            &bin_dir,
            &dst_root,
        )?;

        Ok(())
    }

    fn build_bin(
        &self,
        cargo: &str,
        src_root: impl AsRef<Path>,
        target_root: &str,
    ) -> Result<(), Box<dyn Error>> {
        let src_root = src_root.as_ref().join(OC_BIN);
        let bin_dir = Path::new(target_root).join("x86_64-pc-oceanic");
        let dst_root = Path::new(target_root).join("bootfs/bin");
        let dep_root = Path::new(target_root).join("bootfs/lib");

        let mut dep_lib = ["libldso.so", "libco2.so"]
            .into_iter()
            .map(ToString::to_string)
            .collect::<HashSet<_>>();

        for ent in fs::read_dir(src_root)?.flatten() {
            let ty = ent.file_type()?;
            let name = ent.file_name();
            if ty.is_dir() && name != ".cargo" {
                self.build_impl(cargo, &name, &name, ent.path(), &bin_dir, &dst_root)?;
                for dep in fs::read_dir(bin_dir.join(self.profile()).join("deps"))?.flatten() {
                    let name = dep.file_name();
                    match name.to_str() {
                        Some(name) if name.ends_with(".so") && dep_lib.insert(name.to_string()) => {
                            fs::copy(dep.path(), dep_root.join(name))?;
                            self.gen_debug(name, &dep_root, DEBUG_DIR)?;
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    fn build_impl(
        &self,
        cargo: &str,
        bin_name: impl AsRef<Path>,
        dst_name: impl AsRef<Path>,
        src_dir: impl AsRef<Path>,
        bin_dir: impl AsRef<Path>,
        target_dir: impl AsRef<Path>,
    ) -> Result<(), Box<dyn Error>> {
        println!("Building {:?}", dst_name.as_ref());

        let mut cmd = Command::new(cargo);
        let cmd = cmd.current_dir(src_dir).arg("build");
        if self.release {
            cmd.arg("--release");
        }
        cmd.status()?.exit_ok()?;
        let bin_dir = bin_dir.as_ref().join(self.profile());
        fs::copy(bin_dir.join(bin_name), target_dir.as_ref().join(&dst_name))?;
        self.gen_debug(dst_name, target_dir, DEBUG_DIR)?;
        Ok(())
    }

    fn gen_debug(
        &self,
        target_name: impl AsRef<Path>,
        target_dir: impl AsRef<Path>,
        dbg_dir: impl AsRef<Path>,
    ) -> Result<(), Box<dyn Error>> {
        let target_path = target_dir.as_ref().join(&target_name);
        {
            let mut sym_name = OsString::from(target_name.as_ref().as_os_str());
            sym_name.push(".sym");
            Command::new("llvm-objcopy")
                .arg("--only-keep-debug")
                .arg(&target_path)
                .arg(dbg_dir.as_ref().join(sym_name))
                .status()?;
            Command::new("llvm-objcopy")
                .arg("--strip-debug")
                .arg(&target_path)
                .status()?;
        }
        {
            let mut asm_name = OsString::from(target_name.as_ref().as_os_str());
            asm_name.push(".asm");
            fs::write(
                dbg_dir.as_ref().join(asm_name),
                Command::new("ndisasm")
                    .arg(&target_path)
                    .arg("-b 64")
                    .output()?
                    .stdout,
            )?;
        }
        {
            let mut txt_name = OsString::from(target_name.as_ref().as_os_str());
            txt_name.push(".txt");
            fs::write(
                dbg_dir.as_ref().join(txt_name),
                Command::new("llvm-objdump")
                    .arg("-x")
                    .arg(&target_path)
                    .output()?
                    .stdout,
            )?;
        }
        Ok(())
    }
}

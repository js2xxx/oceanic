use std::{
    collections::HashSet,
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::LazyLock,
};

use anyhow::Context;
use clap::{Parser, Subcommand};
use serde::Deserialize;

use crate::{
    BOOTFS, DEBUG_DIR, H2O_BOOT, H2O_KERNEL, H2O_SYSCALL, H2O_TINIT, OC_BIN, OC_DRV, OC_LIB,
};

static CARGO: LazyLock<OsString> =
    LazyLock::new(|| env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));

static LLVM: LazyLock<OsString> =
    LazyLock::new(|| env::var_os("LLVM_PATH").unwrap_or_else(|| "/usr/lib/llvm-14".into()));

static LLVM_OBJCOPY: LazyLock<PathBuf> =
    LazyLock::new(|| Path::new(&*LLVM).join("bin/llvm-objcopy"));
static LLVM_OBJDUMP: LazyLock<PathBuf> =
    LazyLock::new(|| Path::new(&*LLVM).join("bin/llvm-objdump"));
static LLVM_IFS: LazyLock<PathBuf> = LazyLock::new(|| Path::new(&*LLVM).join("bin/llvm-ifs"));

#[derive(Debug, Subcommand)]
pub enum Type {
    Img,
}

#[derive(Debug, Parser)]
pub struct Dist {
    #[command(subcommand)]
    ty: Type,
    #[arg(long, short)]
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

    pub fn build(self) -> Result<(), anyhow::Error> {
        let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let target_root = env::var("CARGO_TARGET_DIR")
            .unwrap_or_else(|_| src_root.join("target").to_string_lossy().to_string());

        create_dir_all(&target_root, src_root)?;

        // Generate syscall stubs
        crate::gen::gen_syscall(
            src_root.join(H2O_KERNEL).join("syscall"),
            src_root.join(H2O_KERNEL).join("target/wrapper.rs"),
            src_root.join("h2o/libs/syscall/target/call.rs"),
            src_root.join("h2o/libs/syscall/target/stub.rs"),
            src_root.join("h2o/libs/syscall/target/num.rs"),
        )
        .context("failed to generate syscalls")?;

        // Build h2o_boot
        self.build_impl(
            "h2o_boot.efi",
            "BootX64.efi",
            src_root.join(H2O_BOOT),
            Path::new(&target_root).join("x86_64-unknown-uefi"),
            &target_root,
        )
        .context("failed to build h2o_boot")?;

        // Build the VDSO
        self.build_vdso(src_root, &target_root)
            .context("failed to build VDSO")?;

        // Build h2o_kernel
        self.build_impl(
            "h2o",
            "KERNEL",
            src_root.join(H2O_KERNEL),
            Path::new(&target_root).join("x86_64-unknown-none"),
            &target_root,
        )
        .context("failed to build h2o_kernel")?;

        // Build h2o_tinit
        self.build_impl(
            "tinit",
            "TINIT",
            src_root.join(H2O_TINIT),
            Path::new(&target_root).join("x86_64-unknown-none"),
            &target_root,
        )
        .context("failed to build h2o_tinit")?;

        self.build_lib(src_root, &target_root)
            .context("failed to build libraries")?;
        self.build_bin(src_root, &target_root)
            .context("failed to build binaries")?;
        self.build_drv(src_root, &target_root)
            .context("failed to build drivers")?;

        crate::gen::gen_bootfs(Path::new(BOOTFS).join("../BOOT.fs"))
            .context("failed to generate BOOTFS")?;

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

    fn build_vdso(&self, src_root: &Path, target_root: &String) -> Result<(), anyhow::Error> {
        let target_triple = src_root.join(".cargo/x86_64-pc-oceanic.json");
        let cd = src_root.join(H2O_SYSCALL);
        let ldscript = cd.join("syscall.ld");

        println!("Building VDSO");

        let mut cmd = Command::new(&*CARGO);

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
        .status()
        .context("failed to execute cargo")?
        .exit_ok()
        .context("cargo returned error")?;

        let bin_dir = Path::new(target_root).join("x86_64-pc-oceanic/release");
        let path = src_root.join(H2O_KERNEL).join("target/vdso");

        fs::copy(bin_dir.join("libsv_call.so"), &path).context("failed to copy the binary file")?;

        Command::new(&*LLVM_IFS)
            .arg("--input-format=ELF")
            .arg(format!(
                "--output-elf={}/sysroot/usr/lib/libh2o.so",
                target_root
            ))
            .arg(&path)
            .status()
            .context("failed to execute llvm-ifs")?
            .exit_ok()
            .context("llvm-ifs returned error")?;

        let out = Command::new(&*LLVM_OBJDUMP)
            .arg("--syms")
            .arg(&path)
            .output()
            .context("failed to execute llvm-objdump")?
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
        )
        .context("failed to write to constant file")?;

        self.gen_debug("vdso", src_root.join(H2O_KERNEL).join("target"), DEBUG_DIR)
            .context("failed to generate debug info")?;

        Ok(())
    }

    fn build_lib(&self, src_root: impl AsRef<Path>, target_root: &str) -> anyhow::Result<()> {
        let src_root = src_root.as_ref().join(OC_LIB);
        let bin_dir = Path::new(target_root).join("x86_64-pc-oceanic");
        let dst_root = Path::new(target_root).join("bootfs/lib");

        self.build_impl(
            "libldso.so",
            "ld-oceanic.so",
            src_root.join("libc/ldso"),
            &bin_dir,
            &dst_root,
        )
        .context("failed to build LDSO")?;

        Command::new(&*LLVM_IFS)
            .arg("--input-format=ELF")
            .arg(format!(
                "--output-elf={}",
                Path::new(target_root)
                    .join("sysroot/usr/lib/libldso.so")
                    .to_string_lossy()
            ))
            .arg(bin_dir.join(self.profile()).join("libldso.so"))
            .status()
            .context("failed to execute llvm-ifs")?
            .exit_ok()
            .context("llvm-ifs returned error")?;

        self.build_impl(
            "libco2.so",
            "libco2.so",
            src_root.join("libc"),
            &bin_dir,
            &dst_root,
        )
        .context("failed to build libc")?;

        Ok(())
    }

    fn build_bin(&self, src_root: impl AsRef<Path>, target_root: &str) -> anyhow::Result<()> {
        let bin_dir = Path::new(target_root).join("x86_64-pc-oceanic");
        let dep_root = Path::new(target_root).join("bootfs/lib");

        let mut dep_lib = ["libldso.so", "libco2.so"]
            .into_iter()
            .map(ToString::to_string)
            .collect::<HashSet<_>>();

        let src_root = src_root.as_ref().join(OC_BIN);
        let dst_root = Path::new(target_root).join("bootfs/bin");
        for ent in fs::read_dir(src_root)?.flatten() {
            let ty = ent.file_type()?;
            let name = ent.file_name();
            if ty.is_dir() && name != ".cargo" {
                self.gen_manifest(&name, ent.path(), &dst_root)
                    .with_context(|| format!("failed to gen {:?}'s manifest", ent.path()))?;
                self.build_impl(&name, &name, ent.path(), &bin_dir, &dst_root)
                    .with_context(|| format!("failed to build {:?}", ent.path()))?;
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

    fn build_drv(&self, src_root: impl AsRef<Path>, target_root: &str) -> anyhow::Result<()> {
        let bin_dir = Path::new(target_root).join("x86_64-pc-oceanic");
        let dep_root = Path::new(target_root).join("bootfs/lib");

        let mut dep_lib = ["libldso.so", "libco2.so"]
            .into_iter()
            .map(ToString::to_string)
            .collect::<HashSet<_>>();

        let src_root = src_root.as_ref().join(OC_DRV);
        let dst_root = Path::new(target_root).join("bootfs/drv");

        for ent in fs::read_dir(src_root)?.flatten() {
            let ty = ent.file_type()?;
            let name = ent.file_name();
            if ty.is_dir() && name != ".cargo" {
                let dst_name = {
                    let name = name.to_string_lossy().replace('-', "_");
                    let name = "lib".to_string() + &name + ".so";
                    OsString::from(name)
                };
                self.gen_manifest(&dst_name, ent.path(), &dst_root)
                    .with_context(|| format!("failed to gen {:?}'s manifest", ent.path()))?;
                self.build_impl(&dst_name, &dst_name, ent.path(), &bin_dir, &dst_root)
                    .with_context(|| format!("failed to build {:?}", ent.path()))?;
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
        bin_name: impl AsRef<Path>,
        dst_name: impl AsRef<Path>,
        src_dir: impl AsRef<Path>,
        bin_dir: impl AsRef<Path>,
        target_dir: impl AsRef<Path>,
    ) -> anyhow::Result<()> {
        println!("Building {:?}", dst_name.as_ref());

        let mut cmd = Command::new(&*CARGO);
        let cmd = cmd.current_dir(src_dir).arg("build");
        if self.release {
            cmd.arg("--release");
        }
        cmd.status()
            .context("failed to execute cargo")?
            .exit_ok()
            .context("cargo returned error")?;

        let bin_dir = bin_dir.as_ref().join(self.profile());
        fs::copy(bin_dir.join(bin_name), target_dir.as_ref().join(&dst_name))
            .context("failed to copy the binary file")?;

        self.gen_debug(dst_name, target_dir, DEBUG_DIR)
            .context("failed to generate debug info")?;
        Ok(())
    }

    fn gen_debug(
        &self,
        target_name: impl AsRef<Path>,
        target_dir: impl AsRef<Path>,
        dbg_dir: impl AsRef<Path>,
    ) -> anyhow::Result<()> {
        let target_path = target_dir.as_ref().join(&target_name);
        {
            let mut sym_name = OsString::from(target_name.as_ref().as_os_str());
            sym_name.push(".sym");
            Command::new(&*LLVM_OBJCOPY)
                .arg("--only-keep-debug")
                .arg(&target_path)
                .arg(dbg_dir.as_ref().join(sym_name))
                .status()
                .context("failed to extract debug symbols")?;
            Command::new(&*LLVM_OBJCOPY)
                .arg("--strip-debug")
                .arg(&target_path)
                .status()
                .context("failed to strip debug symbols")?;
            // TODO: Check the return status. By far the generated `BOOTX64.efi`
            // contains no symbol info, so no check for now.
        }
        {
            let mut asm_name = OsString::from(target_name.as_ref().as_os_str());
            asm_name.push(".asm");
            fs::write(
                dbg_dir.as_ref().join(asm_name),
                Command::new("ndisasm")
                    .arg(&target_path)
                    .arg("-b 64")
                    .output()
                    .context("failed to disassemble the binary file")?
                    .stdout,
            )
            .context("failed to write to disassembled file")?;
        }
        {
            let mut txt_name = OsString::from(target_name.as_ref().as_os_str());
            txt_name.push(".txt");
            fs::write(
                dbg_dir.as_ref().join(txt_name),
                Command::new(&*LLVM_OBJDUMP)
                    .arg("-x")
                    .arg(&target_path)
                    .output()
                    .context("failed to dump debug info")?
                    .stdout,
            )
            .context("failed to write to debug info file")?;
        }
        Ok(())
    }

    fn gen_manifest(
        &self,
        dst_name: impl AsRef<Path>,
        src_dir: impl AsRef<Path>,
        target_dir: impl AsRef<Path>,
    ) -> Result<(), anyhow::Error> {
        let file = src_dir.as_ref().join("Cargo.toml");
        let content = fs::read_to_string(file).context("failed to read Cargo.toml")?;
        let table = content.parse::<toml::Table>()?;
        let man = table
            .get("package")
            .and_then(|v| v.get("metadata"))
            .and_then(|v| v.get("osc"))
            .context("failed to get `package.metadata.osc` in Cargo.toml")?;

        let comp = osc::Component::deserialize(man.clone())
            .context("failed to deserialize component config")?;

        let cfg = bincode::encode_to_vec(comp, bincode::config::standard())
            .context("failed to generate config file")?;

        let dst_file = target_dir
            .as_ref()
            .join(dst_name.as_ref().with_extension("cfg"));
        fs::write(dst_file, cfg).context("failed to write driver manifest")?;

        Ok(())
    }
}

pub fn create_dir_all(target_root: &String, src_root: &Path) -> Result<(), anyhow::Error> {
    let create_dir = |path: &Path| -> anyhow::Result<()> {
        fs::create_dir_all(path).with_context(|| format!("failed to create dir {path:?}"))
    };
    create_dir(&PathBuf::from(target_root).join("bootfs/lib"))?;
    create_dir(&PathBuf::from(target_root).join("bootfs/drv"))?;
    create_dir(&PathBuf::from(target_root).join("bootfs/bin"))?;
    create_dir(&PathBuf::from(target_root).join("sysroot/usr/include"))?;
    create_dir(&PathBuf::from(target_root).join("sysroot/usr/lib"))?;
    create_dir(&src_root.join(H2O_KERNEL).join("target"))?;
    create_dir(&src_root.join("h2o/libs/syscall/target"))?;
    create_dir("debug".as_ref())?;
    Ok(())
}

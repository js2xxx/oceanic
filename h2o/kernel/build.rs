#[cfg(target_arch = "x86_64")]
fn tram_build() {
      use std::env;
      use std::process::Command;

      let target_dir = env::var("OUT_DIR").unwrap();
      let file = "src/cpu/x86_64/apic/tram.asm";

      println!("cargo:rerun-if-changed={}", file);
      let cmd = Command::new("nasm")
            .args(&[file, "-o", format!("{}/tram", target_dir).as_str()])
            .status()
            .expect("Failed to build the compiling command");

      assert!(cmd.success(), "Failed to compile `tram.asm`");
}

fn main() {
      if cfg!(target_arch = "x86_64") {
            tram_build();
      }
}

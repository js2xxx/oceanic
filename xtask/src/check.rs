use std::{env, error::Error, path::Path, process::Command};

use crate::{H2O_BOOT, H2O_KERNEL, H2O_TINIT};

pub(crate) fn check() -> Result<(), Box<dyn Error>> {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let src_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(1)
        .unwrap();

    {
        // Check h2o_boot
        println!("Checking h2o_boot");

        Command::new(&cargo)
            .current_dir(src_root.join(H2O_BOOT))
            .args(["clippy", "--message-format=json"])
            .status()?
            .exit_ok()?;
    }

    {
        // Check h2o_kernel
        println!("Building h2o_kernel");

        Command::new(&cargo)
            .current_dir(src_root.join(H2O_KERNEL))
            .args(["clippy", "--message-format=json"])
            .status()?
            .exit_ok()?;
    }

    // Build h2o_tinit
    {
        println!("Checking h2o_tinit");

        Command::new(&cargo)
            .current_dir(src_root.join(H2O_TINIT))
            .args(["clippy", "--message-format=json"])
            .status()?
            .exit_ok()?;
    }

    Command::new(&cargo)
        .current_dir(src_root.join("lib"))
        .args(["clippy", "--message-format=json"])
        .status()?
        .exit_ok()?;

    Ok(())
}

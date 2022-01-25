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
        log::info!("Checking h2o_boot");

        Command::new(&cargo)
            .current_dir(src_root.join(H2O_BOOT))
            .args(["clippy", "--message-format=json"])
            .status()?
            .exit_ok()?;
    }

    {
        // Check h2o_kernel
        log::info!("Building h2o_kernel");

        Command::new(&cargo)
            .current_dir(src_root.join(H2O_KERNEL))
            .args(["clippy", "--message-format=json"])
            .status()?
            .exit_ok()?;
    }

    // Build h2o_tinit
    {
        log::info!("Checking h2o_tinit");

        Command::new(&cargo)
            .current_dir(src_root.join(H2O_TINIT))
            .args(["clippy", "--message-format=json"])
            .status()?
            .exit_ok()?;
    }

    Ok(())
}

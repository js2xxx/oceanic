use anyhow::{Context, Error};
use osc::Component;
use solvent_rpc::{io::OpenOptions, sync::Client};
use solvent_std::path::Path;

pub async fn get_boot_coms() -> anyhow::Result<()> {
    for entry in solvent_fs::read_dir("boot/bin")
        .map_err(Error::msg)
        .context("failed to read boot/bin")?
    {
        let entry = entry
            .map_err(Error::msg)
            .context("failed to enumerate boot/bin")?;
        if entry.name.ends_with(".cfg") {
            log::debug!("{}", entry.name);
            let file = {
                let client =
                    solvent_fs::open(Path::new("boot/bin").join(&entry.name), OpenOptions::READ)
                        .map_err(Error::msg)
                        .context("failed to open cfg file")?;
                let client = client
                    .into_async()
                    .map_err(|_| Error::msg("failed to get async file client"))?;
                client
                    .read(entry.metadata.len)
                    .await
                    .map_err(Error::msg)
                    .context("failed to RPC")?
                    .map_err(Error::msg)
                    .context("failed to read file")?
            };
            let (cfg, _) =
                bincode::decode_from_slice::<Component, _>(&file, bincode::config::standard())
                    .map_err(Error::msg)
                    .context("failed to parse cfg file")?;
            log::debug!("{cfg:?}");
        }
    }

    Ok(())
}

#![no_std]

use solvent::prelude::Channel;

extern crate alloc;

async fn init(_driver_instance: Channel) {
    log::debug!("Hello from driver");
}

solvent_ddk::entry!(init);

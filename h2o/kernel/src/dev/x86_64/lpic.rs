#![allow(dead_code)]

use archop::io::{Io, Port};

const MASTER_PORT: u16 = 0x20;
const SLAVE_PORT: u16 = 0xA0;

unsafe fn read_cmd(chip: &Port<u8>) -> u8 {
    chip.read()
}

unsafe fn write_cmd(chip: &mut Port<u8>, value: u8) {
    chip.write(value)
}

unsafe fn read_data(chip: &Port<u8>) -> u8 {
    chip.read_offset(1)
}

unsafe fn write_data(chip: &mut Port<u8>, value: u8) {
    chip.write_offset(1, value)
}

struct LegacyPic {
    master: Port<u8>,
    slave: Port<u8>,
    masked_irq: u16,
}

impl LegacyPic {
    pub fn new() -> Self {
        LegacyPic {
            // SAFETY: These ports are valid and present.
            master: unsafe { Port::new(MASTER_PORT) },
            slave: unsafe { Port::new(SLAVE_PORT) },
            masked_irq: 0,
        }
    }

    /// Shut down the chips due to another alternate interrupt method (I/O
    /// APIC).
    ///
    /// # Safety
    ///
    /// The caller must ensure that its called only once.
    pub unsafe fn init_masked(&mut self) {
        write_data(&mut self.master, 0xFF);
        write_data(&mut self.slave, 0xFF);
    }

    /// Initialize the Legacy PIC in case of lack of other interrupt methods.
    ///
    /// # Safety
    ///
    /// The caller must ensure that its called only once.
    pub unsafe fn init(&mut self) {
        todo!()
    }
}

/// Initialize Legacy PIC (8259A).
///
/// # Safety
///
/// This function must be called only once from the bootstrap CPU.
pub unsafe fn init(masked: bool) {
    let mut lpic = LegacyPic::new();
    if masked {
        lpic.init_masked();
    } else {
        lpic.init();
    }
}

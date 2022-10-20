#![no_std]

use core::fmt::{self, Write};

use solvent::prelude::Instant;
use spin::Mutex;

fn cur_cpu() -> usize {
    let mut ret;
    unsafe { core::arch::asm!("rdtscp", out("rcx") ret, options(nostack)) };
    ret
}

static LOGGER: Logger = Logger;

const BUFFER_SIZE: usize = 256;

static BUFFER: Mutex<Buffer> = Mutex::new(Buffer([0; BUFFER_SIZE], 0));

struct Buffer([u8; BUFFER_SIZE], usize);

struct Logger;

impl Write for Buffer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        self.0
            .get_mut(self.1..)
            .and_then(|buf| buf.get_mut(..bytes.len()))
            .map_or(Err(fmt::Error), |buf| {
                buf.copy_from_slice(bytes);
                self.1 += bytes.len();
                Ok(())
            })
    }
}

impl log::Log for Logger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let cur_time = Instant::now();
        let mut buffer = BUFFER.lock();
        if record.level() < log::Level::Debug {
            write!(
                &mut *buffer,
                "[{}] {}: {}",
                cur_time,
                record.level(),
                record.args()
            )
        } else {
            let file = record.file().unwrap_or("<NULL>");
            let line = record.line().unwrap_or(0);
            write!(
                &mut *buffer,
                "[{}] {}: [#{} {}:{}] {}",
                cur_time,
                record.level(),
                cur_cpu(),
                file,
                line,
                record.args(),
            )
        }
        .expect("Failed to write str");
        let _ = unsafe { sv_call::sv_log(buffer.0.as_ptr(), buffer.1) };
        *buffer = Buffer([0; BUFFER_SIZE], 0);
        drop(buffer);
    }

    fn flush(&self) {}
}

pub fn init(max_level: log::Level) {
    log::set_logger(&LOGGER).expect("Failed to set the logger");
    log::set_max_level(max_level.to_level_filter());
}

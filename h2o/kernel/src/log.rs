mod serial;

use core::fmt::*;
use core::mem::MaybeUninit;
use spin::Mutex;

struct Logger {
      spout: Mutex<serial::SPOut>,
      level: log::Level,
}

impl Logger {
      pub fn new(level: log::Level) -> Logger {
            Logger {
                  spout: Mutex::new(serial::SPOut::new()),
                  level,
            }
      }
}

impl log::Log for Logger {
      #[inline]
      fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= self.level
      }

      fn log(&self, record: &log::Record) {
            if !self.enabled(record.metadata()) {
                  return;
            }

            let mut os = self.spout.lock();

            let res = if record.level() < log::Level::Debug {
                  write(
                        &mut *os,
                        format_args!("{}: {}", record.level(), record.args()),
                  )
            } else {
                  write(
                        &mut *os,
                        format_args!(
                              "{}: {:?}: {:?}: {}",
                              record.level(),
                              record.file(),
                              record.line(),
                              record.args()
                        ),
                  )
            };
            res.expect("Failed to output");
      }

      #[inline]
      fn flush(&self) {}
}

static mut LOGGER: MaybeUninit<Logger> = MaybeUninit::uninit();

pub fn init() {
      let max_level = log::Level::Debug;
      unsafe {
            LOGGER.as_mut_ptr().write(Logger::new(max_level));
            log::set_logger(&*LOGGER.as_ptr()).expect("Failed to set the logger");
      }
      log::set_max_level(max_level.to_level_filter());
}

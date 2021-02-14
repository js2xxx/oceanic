mod serial;

use core::fmt::*;
use core::mem::MaybeUninit;
use spin::Mutex;

struct Logger {
      spout: Mutex<serial::SPOut>,
      level: log::Level,
}

struct OptionU32Display(Option<u32>);

impl core::fmt::Display for OptionU32Display {
      fn fmt(&self, f: &mut Formatter<'_>) -> Result {
            if let Some(val) = self.0 {
                  write!(f, "{}", val)
            } else {
                  write!(f, "<NULL>")
            }
      }
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

            let res = if record.level() <= log::Level::Debug {
                  write(
                        &mut *os,
                        format_args!("{}: {}\n", record.level(), record.args()),
                  )
            } else {
                  let file = record.file().unwrap_or("<NULL>");
                  let line = OptionU32Display(record.line());
                  write(
                        &mut *os,
                        format_args!(
                              "{}: {}: {}: {}\n",
                              record.level(),
                              file,
                              line,
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

pub fn init(max_level: log::Level) {
      unsafe {
            LOGGER.as_mut_ptr().write(Logger::new(max_level));
            log::set_logger(&*LOGGER.as_ptr()).expect("Failed to set the logger");
      }
      log::set_max_level(max_level.to_level_filter());
}

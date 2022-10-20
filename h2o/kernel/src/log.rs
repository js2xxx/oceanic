pub mod flags;
mod serial;

use core::{
    fmt::*,
    mem::MaybeUninit,
    sync::atomic::{AtomicBool, Ordering::*},
};

use spin::Mutex;

pub use self::serial::COM_LOG;
use crate::{cpu::time::Instant, sched::PREEMPT};

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

pub static HAS_TIME: AtomicBool = AtomicBool::new(false);

struct Logger {
    output: Mutex<serial::Output>,
    level: log::Level,
}

impl Logger {
    pub fn new(level: log::Level) -> Logger {
        Logger {
            output: Mutex::new(unsafe { serial::Output::new(COM_LOG) }),
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

        let _pree = PREEMPT.lock();
        let mut os = self.output.lock();
        let cur_time = HAS_TIME
            .load(Acquire)
            .then(Instant::now)
            .unwrap_or(unsafe { Instant::from_raw(0) });

        let res = if record.level() < log::Level::Debug {
            writeln!(*os, "[{}] {}: {}", cur_time, record.level(), record.args(),)
        } else {
            let file = record.file().unwrap_or("<NULL>");
            let line = OptionU32Display(record.line());
            writeln!(
                *os,
                "[{}] {}: [#{} {}:{}] {}",
                cur_time,
                record.level(),
                unsafe { crate::cpu::id() },
                file,
                line,
                record.args(),
            )
        };
        res.expect("Failed to output");
    }

    #[inline]
    fn flush(&self) {}
}

static mut LOGGER: MaybeUninit<Logger> = MaybeUninit::uninit();

/// # Safety
///
/// This function should only be called once before everything else is to be
/// started up.
pub unsafe fn init(max_level: log::Level) {
    let logger = LOGGER.write(Logger::new(max_level));
    log::set_logger(logger).expect("Failed to set the logger");
    log::set_max_level(max_level.to_level_filter());
}

mod syscall {
    use core::fmt::Write;

    use sv_call::*;

    use super::LOGGER;
    use crate::{
        sched::PREEMPT,
        syscall::{In, UserPtr},
    };

    #[syscall]
    fn log(buffer: UserPtr<In>, len: usize) -> Result {
        buffer.check_slice(len)?;
        let string =
            core::str::from_utf8(unsafe { core::slice::from_raw_parts(buffer.as_ptr(), len) })?;
        let _pree = PREEMPT.lock();
        let mut os = unsafe { LOGGER.assume_init_ref() }.output.lock();
        writeln!(os, "{}", string).map_err(|_| EFAULT)?;
        Ok(())
    }
}

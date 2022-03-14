static LOGGER: Logger = Logger;

struct Logger;

impl log::Log for Logger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let _ = unsafe { sv_call::call::sv_log(record as *const _ as *const _) };
    }

    fn flush(&self) {}
}

pub fn init(max_level: log::Level) {
    log::set_logger(&LOGGER).expect("Failed to set the logger");
    log::set_max_level(max_level.to_level_filter());
}

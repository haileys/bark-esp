use core::ffi::{CStr, c_char};

use esp_idf_sys::{esp_log_level_t, esp_log_level_set, esp_log_timestamp, esp_log_level_t_ESP_LOG_VERBOSE};
use log::Level;
use cstr::cstr;
use core::fmt::Write;

pub fn init() {
    static LOG: EspLog = EspLog;
    log::set_logger(&LOG).expect("init logger");
    log::set_max_level(log::LevelFilter::Trace);
}

extern "C" {
    fn esp_log_write(
        level: esp_log_level_t,
        tag: *const c_char,
        fmt: *const c_char,
        arg: *const c_char,
    );
}

struct EspLog;

impl log::Log for EspLog {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        esp_println::println!("EspLog::log");

        let color = log_color(record.level());
        let label = log_label(record.level());
        let target = record.target();
        let timestamp = unsafe { esp_log_timestamp() };
        let reset = COLOR_RESET;

        let mut buffer = Buffer::<300>::new();
        let _ = write!(
            &mut buffer,
            "{color}{label} [{timestamp:>10}] {target}: {}{reset}",
            record.args(),
        );

        static DYNAMIC_TAG: &CStr = cstr!("rust-dynamic");

        esp_println::println!("about to log...");

        unsafe {
            esp_log_write(
                log_level(record.level()),
                DYNAMIC_TAG.as_ptr(),
                cstr!("%s\n").as_ptr(),
                buffer.as_cstr().as_ptr(),
            );
        }
    }

    fn flush(&self) {}
}

struct Buffer<const SIZE: usize> {
    len: usize,
    buff: [u8; SIZE],
}

impl<const SIZE: usize> Buffer<SIZE> {
    pub fn new() -> Self {
        Buffer { len: 0, buff: [0u8; SIZE] }
    }

    pub fn as_cstr(&self) -> &CStr {
        unsafe {
            CStr::from_bytes_with_nul_unchecked(self.as_bytes_with_nul())
        }
    }

    fn unused_mut(&mut self) -> &mut [u8] {
        &mut self.buff[self.len..]
    }

    fn as_bytes_with_nul(&self) -> &[u8] {
        &self.buff[0..(self.len + 1)]
    }
}

impl<const SIZE: usize> Write for Buffer<SIZE> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let unused = self.unused_mut();
        let len = core::cmp::min(s.len(), unused.len() - 1);
        unused[0..len].copy_from_slice(&s.as_bytes()[0..len]);
        unused[len] = 0;
        self.len += len;
        Ok(())
    }
}

// taken from esp_log.h:

const COLOR_RESET: &'static str = "\x1b[0m";

fn log_level(level: Level) -> esp_log_level_t {
    // we don't get the constants in bindings for some reason, hardcode:
    match level {
        Level::Error => esp_idf_sys::esp_log_level_t_ESP_LOG_ERROR,
        Level::Warn  => esp_idf_sys::esp_log_level_t_ESP_LOG_WARN,
        Level::Info  => esp_idf_sys::esp_log_level_t_ESP_LOG_INFO,
        Level::Debug => esp_idf_sys::esp_log_level_t_ESP_LOG_DEBUG,
        Level::Trace => esp_idf_sys::esp_log_level_t_ESP_LOG_VERBOSE,
    }
}

fn log_color(level: Level) -> &'static str {
    match level {
        Level::Error => "\x1b[0;31m",
        Level::Warn  => "\x1b[0;33m",
        Level::Info  => "\x1b[0;32m",
        Level::Debug => "",
        Level::Trace => "",
    }
}

fn log_label(level: Level) -> &'static str {
    match level {
        Level::Error => "E",
        Level::Warn  => "W",
        Level::Info  => "I",
        Level::Debug => "D",
        Level::Trace => "T",
    }
}

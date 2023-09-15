use core::ffi::{CStr, c_char};

use esp_idf_sys::{esp_log_level_t, esp_log_timestamp};
use log::Level;
use cstr::cstr;
use core::fmt::Write;

pub fn init() {
    static LOG: EspLog = EspLog;
    log::set_logger(&LOG).expect("init logger");
    log::set_max_level(log::LevelFilter::Trace);
}

const RESET: &str = "\x1b[0m";
const BOLD_ON: &str = "\x1b[1m";
const BOLD_OFF: &str = "\x1b[21m";

struct EspLog;

impl log::Log for EspLog {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let color = log_color(record.level());
        let label = log_label(record.level());
        let target = record.target();
        let timestamp = unsafe { esp_log_timestamp() };

        let mut buffer = Buffer::<300>::new();
        let _ = write!(
            &mut buffer,
            "{c_normal}{label} [{timestamp:>10}] {c_bright}{target}:{RESET}{c_normal} {}{RESET}",
            record.args(),
            c_normal = color.normal,
            c_bright = color.bright,
        );

        let tag = static_str(record.target())
            .unwrap_or("rust-dynamic");

        esp_log(record.level(), tag, buffer.as_cstr());
    }

    fn flush(&self) {}
}

fn esp_log(level: log::Level, tag: &'static str, message: &CStr) {
    extern "C" {
        fn esp_log_write(
            level: esp_log_level_t,
            tag: *const u8,
            fmt: *const c_char,
            arg: *const c_char,
        );
    }

    unsafe {
        esp_log_write(
            log_level(level),
            tag.as_ptr(),
            cstr!("%s\n").as_ptr(),
            message.as_ptr(),
        );
    }
}

fn static_str(string: &str) -> Option<&'static str> {
    extern "C" {
        static _rodata_start: u8;
        static _rodata_end: u8;
    }

    let rodata_start: *const u8 = unsafe { &_rodata_start };
    let rodata_end: *const u8 = unsafe { &_rodata_end };

    let ptr = string.as_ptr();

    if rodata_start <= ptr && ptr < rodata_end {
        Some(unsafe {
            core::mem::transmute::<&str, &'static str>(string)
        })
    } else {
        None
    }
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

struct Color {
    normal: &'static str,
    bright: &'static str,
}

fn log_color(level: Level) -> Color {
    match level {
        Level::Error => Color { normal: "\x1b[31m", bright: "\x1b[1;31m" },
        Level::Warn  => Color { normal: "\x1b[33m", bright: "\x1b[1;33m" },
        Level::Info  => Color { normal: "\x1b[32m", bright: "\x1b[1;32m" },
        Level::Debug => Color { normal: "", bright: "" },
        Level::Trace => Color { normal: "\x1b[2m", bright: "\x1b[2m" },
    }
}

fn log_label(level: Level) -> &'static str {
    match level {
        Level::Error => "ERROR",
        Level::Warn  => "WARN ",
        Level::Info  => "INFO ",
        Level::Debug => "DEBUG",
        Level::Trace => "TRACE",
    }
}

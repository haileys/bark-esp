use core::ffi::c_int;

use esp_idf_sys::{esp_vfs_dev_uart_use_driver, esp, uart_driver_install};

const UART_NUM: c_int = 1;
// needs to be larger for the logo:
const UART_BUFFER_SIZE: c_int = 1500;
const UART_QUEUE_SIZE: c_int = 10;

pub(super) fn init_uart0() {
    // Enable UART0 driver so stdin can be read.
    unsafe {
        esp!(uart_driver_install(
            UART_NUM,
            UART_BUFFER_SIZE,
            UART_BUFFER_SIZE,
            UART_QUEUE_SIZE,
            core::ptr::null_mut(),
            0
        ))
        .expect("unable to initialize UART driver");
        esp_vfs_dev_uart_use_driver(UART_NUM);
    }
}

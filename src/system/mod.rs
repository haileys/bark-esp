pub mod eventgroup;
pub mod heap;
pub mod log;
pub mod logo;
pub mod panic;
pub mod rt;
pub mod sema;
pub mod task;
pub mod uart;

/// Call once only
pub unsafe fn init() {
    // init uart and log first
    uart::init_uart0();
    log::init();

    // then sync primitives
    crate::sync::signal::init();

    // then runtime
    rt::init();

    // say hello :)
    esp_println::print!("{}", logo::LOGO);
}

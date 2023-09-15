pub mod event;
pub mod eventloop;
pub mod heap;
pub mod log;
pub mod logo;
// pub mod macros;
pub mod network;
pub mod nvs;
pub mod panic;
pub mod task;
pub mod uart;
pub mod wifi;

/// Call once only
pub unsafe fn init() {
    // init uart and log first
    uart::init_uart0();
    log::init();

    esp_println::print!("{}", logo::LOGO);

    eventloop::init();
    nvs::init();
    wifi::init();
    network::init();
}

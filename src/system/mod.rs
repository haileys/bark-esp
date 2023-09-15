pub mod log;
// pub mod sched;
pub mod task;
pub mod uart;

/// Call once only
pub unsafe fn init() {
    uart::init_uart0();
    log::init();
}

#![no_std]
#![no_main]

mod system;

#[no_mangle]
pub unsafe extern "C" fn app_main() {
    system::init();
    main();
}

fn main() {
    log::info!("hello world from esp!");
    system::task::log_tasks();
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { esp_idf_sys::abort(); }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    log::error!("PANIC: {info}");
    unsafe { esp_idf_sys::abort(); }
}

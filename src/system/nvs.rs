use esp_idf_sys as sys;

pub unsafe fn init() {
    if let Err(e) = sys::esp!(sys::nvs_flash_init()) {
        log::warn!("nvs_flash_init failed: {e:?}");
    }
}

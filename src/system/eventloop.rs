use esp_idf_sys as sys;

pub unsafe fn init() {
    if let Err(e) = sys::esp!(sys::esp_event_loop_create_default()) {
        log::error!("esp_event_loop_create_default failed: {e:?}");
    }
}

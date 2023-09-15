use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};

use esp_idf_sys as sys;

pub unsafe fn init() {
    if let Err(e) = sys::esp!(sys::esp_netif_init()) {
        log::error!("esp_netif_init failed: {e:?}");
    }
}

static DEFAULT_NETIF_EXIST: AtomicBool = AtomicBool::new(false);

pub struct Netif {
    ptr: *mut sys::esp_netif_t,
}

impl Netif {
    pub fn create_default_station() -> Self {
        let in_use = DEFAULT_NETIF_EXIST.swap(true, Ordering::SeqCst);
        if in_use {
            panic!("default station netif already exists");
        }

        Netif {
            ptr: unsafe { sys::esp_netif_create_default_wifi_sta() },
        }
    }
}

impl Drop for Netif {
    fn drop(&mut self) {
        unsafe {
            sys::esp_netif_destroy_default_wifi(self.ptr as *mut c_void);
            DEFAULT_NETIF_EXIST.store(false, Ordering::SeqCst);
        }
    }
}

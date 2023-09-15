use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};

use derive_more::From;
use esp_idf_sys as sys;

use super::event::{self, EventHandler, AttachHandlerError};

pub unsafe fn init() {
    if let Err(e) = sys::esp!(sys::esp_netif_init()) {
        log::error!("esp_netif_init failed: {e:?}");
    }
}

static DEFAULT_NETIF_EXIST: AtomicBool = AtomicBool::new(false);

pub struct Netif {
    _event: EventHandler,
}

#[derive(Debug, From)]
pub enum NetifError {
    InUse,
    AttachEvent(AttachHandlerError),
}

impl Netif {
    pub fn create_default_station() -> Result<Self, NetifError> {
        let in_use = DEFAULT_NETIF_EXIST.swap(true, Ordering::SeqCst);
        if in_use {
            return Err(NetifError::InUse);
        }

        let ptr = unsafe { NetifPtr(sys::esp_netif_create_default_wifi_sta()) };
        let event = handle_events(ptr)?;
        Ok(Netif { _event: event })
    }
}

fn handle_events(_: NetifPtr) -> Result<EventHandler, AttachHandlerError> {
    let ip_event = unsafe { sys::IP_EVENT };

    event::attach(ip_event, |message, _data| {
        match message as u32 {
            sys::ip_event_t_IP_EVENT_STA_GOT_IP => {
                log::info!("Got IP!");
            }
            _ => {}
        }
    })
}

pub struct NetifPtr(*mut sys::esp_netif_t);

impl Drop for NetifPtr {
    fn drop(&mut self) {
        unsafe {
            sys::esp_netif_destroy_default_wifi(self.0 as *mut c_void);
            DEFAULT_NETIF_EXIST.store(false, Ordering::SeqCst);
        }
    }
}

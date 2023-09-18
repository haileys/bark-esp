use core::{mem::MaybeUninit, ffi::CStr};
use core::ffi::c_void;

use derive_more::From;
use esp_idf_sys as sys;
use sys::EspError;

use crate::system::heap::{HeapBox, MallocError, UntypedHeapBox};

pub trait Event: Sized {
    unsafe fn event_base() -> sys::esp_event_base_t;
    unsafe fn from_raw(id: u32, data: *mut c_void) -> Option<Self>;
}

pub struct EventHandler {
    event_base: sys::esp_event_base_t,
    instance: sys::esp_event_handler_instance_t,
    _handler: UntypedHeapBox,
}

impl EventHandler {
    pub fn leak(self) {
        core::mem::forget(self)
    }
}

#[derive(Debug, From)]
pub enum AttachHandlerError {
    RegisterEventHandler(EspError),
    AllocateClosure(MallocError),
}

pub fn attach<F: FnMut(i32, *mut c_void) + 'static>(
    event_base: sys::esp_event_base_t,
    handler: F,
) -> Result<EventHandler, AttachHandlerError> {
    let handler = HeapBox::alloc(handler)?;

    let mut instance = MaybeUninit::uninit();

    let instance = unsafe {
        sys::esp!(sys::esp_event_handler_instance_register(
            event_base,
            sys::ESP_EVENT_ANY_ID,
            Some(dispatch::<F>),
            handler.as_mut_ptr() as *mut c_void,
            instance.as_mut_ptr(),
        ))?;

        instance.assume_init()
    };

    log::info!("Attached handler for event {}",
        event_base_as_str(event_base));

    Ok(EventHandler {
        event_base,
        instance,
        _handler: handler.erase_type(),
    })
}

fn event_base_as_str(event_base: sys::esp_event_base_t) -> &'static str {
    unsafe { CStr::from_ptr(event_base) }.to_str().unwrap_or_default()
}

/// SAFETY: must only called by event loop so mut refs don't alias
unsafe extern "C" fn dispatch<F: FnMut(i32, *mut c_void) + 'static>(
    ptr: *mut c_void,
    _: sys::esp_event_base_t,
    event_id: i32,
    event_data: *mut c_void,
) {
    let ptr = ptr as *mut F;
    let func = &mut *ptr;
    (func)(event_id, event_data);
}

impl Drop for EventHandler {
    fn drop(&mut self) {
        unsafe {
            sys::esp_event_handler_instance_unregister(
                self.event_base,
                sys::ESP_EVENT_ANY_ID,
                self.instance,
            );

            log::info!("Detached handler for event {}",
                event_base_as_str(self.event_base));
        }
    }
}

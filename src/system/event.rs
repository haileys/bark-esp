use core::mem::MaybeUninit;
use core::ffi::c_void;

use derive_more::From;
use esp_idf_sys as sys;
use sys::EspError;

use super::heap::{HeapBox, MallocError, UntypedHeapBox};

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

    log::info!("Attached event");

    Ok(EventHandler {
        event_base,
        instance,
        _handler: handler.erase_type(),
    })
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

            log::info!("Unregistered event");
        }
    }
}

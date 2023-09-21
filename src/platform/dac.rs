//! Driver for the onboard DAC
//!
//! Only supports 8 bit output.

use core::{ptr::NonNull, mem::MaybeUninit, ffi::c_void, task::{Context, Poll}};

use derive_more::From;
use esp_idf_sys as sys;
use sys::EspError;

use crate::{system::heap::{HeapBox, MallocError, UntypedHeapBox}, sync::queue::{QueueSender, self, QueueReceiver, AllocQueueError}};

const DMA_BUFFER_COUNT: usize = 4;
const DMA_BUFFER_SIZE: usize = 2048;

pub struct Dac {
    handle: sys::dac_continuous_handle_t,
    state: HeapBox<CallbackState>,
    channel: QueueReceiver<EventData>,
}

#[derive(Debug, From)]
pub struct DacError(sys::EspError);

#[derive(Debug, From)]
pub enum NewDacError {
    AllocCallback(MallocError),
    AllocBufferQueue(AllocQueueError),
    Dac(sys::EspError),
}

impl Dac {
    pub fn new() -> Result<Dac, NewDacError> {
        let config = sys::dac_continuous_config_t {
            chan_mask: sys::dac_channel_mask_t_DAC_CHANNEL_MASK_ALL,
            desc_num: DMA_BUFFER_COUNT as u32,
            buf_size: DMA_BUFFER_SIZE,
            freq_hz: 48000,
            offset: 0,
            clk_src: sys::soc_periph_dac_digi_clk_src_t_DAC_DIGI_CLK_SRC_APLL,
            chan_mode: sys::dac_continuous_channel_mode_t_DAC_CHANNEL_MODE_ALTER,
        };

        let (channel_tx, channel_rx) = queue::channel(DMA_BUFFER_COUNT)?;

        let state = HeapBox::alloc(CallbackState { channel: channel_tx })?;

        let handle = unsafe {
            let mut handle = MaybeUninit::uninit();

            sys::esp!(sys::dac_continuous_new_channels(
                &config,
                handle.as_mut_ptr(),
            ))?;

            handle.assume_init()
        };

        // construct Dac object here so it gets dropped and frees its
        // resource if anything goes wrong from here
        let dac = Dac { handle, state, channel: channel_rx };

        let callbacks = sys::dac_event_callbacks_t {
            on_convert_done: Some(on_convert_done),
            on_stop: None,
        };

        unsafe {
            sys::esp!(sys::dac_continuous_register_event_callback(
                dac.handle,
                &callbacks,
                HeapBox::as_borrowed_mut_ptr(&dac.state) as *mut c_void,
            ))?;
        }

        Ok(dac)
    }

    pub fn enable(&mut self) -> Result<(), sys::EspError> {
        unsafe {
            sys::esp!(sys::dac_continuous_enable(self.handle))
        }
    }

    pub fn disable(&mut self) -> Result<(), sys::EspError> {
        unsafe {
            sys::esp!(sys::dac_continuous_disable(self.handle))
        }
    }

    pub fn start_async_writing(&mut self) -> Result<(), DacError> {
        unsafe {
            sys::esp!(sys::dac_continuous_start_async_writing(self.handle))?;
        }
        Ok(())
    }

    pub fn stop_async_writing(&mut self) -> Result<(), DacError> {
        unsafe {
            sys::esp!(sys::dac_continuous_start_async_writing(self.handle))?;
        }
        Ok(())
    }

    pub fn poll_acquire_buffer(&mut self, cx: &Context) -> Poll<EventData> {
        self.channel.poll_receive(cx)
    }
}

impl Drop for Dac {
    fn drop(&mut self) {
        unsafe {
            sys::dac_continuous_del_channels(self.handle);
        }
    }
}

struct CallbackState {
    channel: QueueSender<EventData>,
}

pub struct EventData(sys::dac_event_data_t);

unsafe impl Send for EventData {}

unsafe extern "C" fn on_convert_done(
    handle: sys::dac_continuous_handle_t,
    event: *const sys::dac_event_data_t,
    state: *mut c_void,
) -> bool {
    let state = state as *mut CallbackState;
    let state = &mut *state;

    let result = state.channel.send_from_isr(EventData(*event));

    result.need_wake
}

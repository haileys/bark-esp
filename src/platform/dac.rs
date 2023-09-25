//! Driver for the onboard DAC
//!
//! Only supports 8 bit output.

use core::ffi::c_void;
use core::future::poll_fn;
use core::mem::MaybeUninit;
use core::slice;
use core::task::{Context, Poll};

use derive_more::From;
use esp_idf_sys as sys;

use crate::sync::ringbuffer::RingBuffer;
use crate::system::task::TaskWakerSet;

const DMA_BUFFER_COUNT: usize = 2;
pub const DMA_BUFFER_SIZE: usize = 2048;

static BUFFER: RingBuffer<u8, 4096> = RingBuffer::new();
static WAKER: TaskWakerSet = TaskWakerSet::new();

pub struct Dac {
    handle: sys::dac_continuous_handle_t,
}

#[derive(Debug, From)]
pub struct DacError(sys::EspError);

#[derive(Debug, From)]
pub struct NewDacError(pub sys::EspError);

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
        let dac = Dac { handle };

        // reset the sample buffer
        // SAFETY: dac_continuous_new_channels ensures that at most one DAC
        // instance exists at any given time, at this point there cannot be
        // any readers or writers
        unsafe { BUFFER.reset(); }

        let callbacks = sys::dac_event_callbacks_t {
            on_convert_done: Some(on_convert_done),
            on_stop: None,
        };

        unsafe {
            sys::esp!(sys::dac_continuous_register_event_callback(
                dac.handle,
                &callbacks,
                core::ptr::null_mut(),
            ))?;
        }

        Ok(dac)
    }

    pub fn enable(&mut self) -> Result<(), DacError> {
        let rc = unsafe { sys::dac_continuous_enable(self.handle) };
        sys::esp!(rc).map_err(DacError)
    }

    #[allow(unused)]
    pub fn disable(&mut self) -> Result<(), DacError> {
        let rc = unsafe { sys::dac_continuous_disable(self.handle) };
        sys::esp!(rc).map_err(DacError)
    }

    pub fn start_async_writing(&mut self) -> Result<(), DacError> {
        let rc = unsafe { sys::dac_continuous_start_async_writing(self.handle) };
        sys::esp!(rc).map_err(DacError)
    }

    #[allow(unused)]
    pub fn stop_async_writing(&mut self) -> Result<(), DacError> {
        let rc = unsafe { sys::dac_continuous_stop_async_writing(self.handle) };
        sys::esp!(rc).map_err(DacError)
    }

    fn poll_write(&mut self, cx: &Context, data: &[u8]) -> Poll<usize> {
        // assert data is full frames:
        assert!(data.len() % 2 == 0);

        // SAFETY: 1. at most one DAC instance exists at any given time
        //         2. we have a mut ref to the one that exists now
        //         3. ergo, we are the only writer
        let nbytes = unsafe { BUFFER.write(data) };

        if nbytes == 0 {
            WAKER.add_task(cx);
            Poll::Pending
        } else {
            Poll::Ready(nbytes)
        }
    }

    pub async fn write(&mut self, mut data: &[u8]) -> Result<(), DacError> {
        while data.len() > 0 {
            let n = poll_fn(|cx| self.poll_write(cx, data)).await;
            // log::info!("wrote {n} bytes to streambuffer");
            data = &data[n..];
        }

        Ok(())
    }
}

impl Drop for Dac {
    fn drop(&mut self) {
        unsafe {
            sys::dac_continuous_del_channels(self.handle);
        }
    }
}

pub struct DmaBuffer(sys::dac_event_data_t);
unsafe impl Send for DmaBuffer {}

unsafe extern "C" fn on_convert_done(
    _handle: sys::dac_continuous_handle_t,
    event: *const sys::dac_event_data_t,
    _state: *mut c_void,
) -> bool {
    let event = &*event;

    // take mut slice ref to the output buffer:
    let output = slice::from_raw_parts_mut(event.buf.cast::<u8>(), event.buf_size);

    // read from the ring buffer into it:
    // SAFETY: ISR is the only reader
    let nbytes = BUFFER.read(output);

    // zero any remaining buffer if we read short
    output[nbytes..].fill(0);

    // notify writers that they can poll again:
    let result = WAKER.wake_from_isr();

    // return need wake flag:
    result.need_wake
}

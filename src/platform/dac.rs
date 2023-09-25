//! Driver for the onboard DAC
//!
//! Only supports 8 bit output.

use core::ffi::c_void;
use core::future::poll_fn;
use core::mem::{MaybeUninit, size_of};
use core::{slice, cmp};
use core::task::{Context, Poll};

use derive_more::From;
use esp_idf_sys as sys;

use crate::stats::STATS;
use crate::sync::ringbuffer::RingBuffer;
use crate::system::task::TaskWakerSet;

const DMA_BUFFER_COUNT: usize = 4;
pub const DMA_BUFFER_SIZE: usize = 512;

#[derive(Clone, Copy, Default)]
#[repr(packed)]
pub struct Frame(pub i8, pub i8);

static BUFFER: RingBuffer<Frame, 512> = RingBuffer::new_with_buffer([Frame(0, 0); 512]);
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
        unsafe {
            BUFFER.reset();
            BUFFER.fill(Frame::default());
        }

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

    pub fn disable(&mut self) -> Result<(), DacError> {
        let rc = unsafe { sys::dac_continuous_disable(self.handle) };
        sys::esp!(rc).map_err(DacError)
    }

    pub fn start_async_writing(&mut self) -> Result<(), DacError> {
        let rc = unsafe { sys::dac_continuous_start_async_writing(self.handle) };
        sys::esp!(rc).map_err(DacError)
    }

    pub fn stop_async_writing(&mut self) -> Result<(), DacError> {
        let rc = unsafe { sys::dac_continuous_stop_async_writing(self.handle) };
        sys::esp!(rc).map_err(DacError)
    }

    fn poll_write(&mut self, cx: &Context, data: &[Frame]) -> Poll<usize> {
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

    pub async fn write(&mut self, mut data: &[Frame]) -> Result<(), DacError> {
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
        let _ = self.stop_async_writing();
        let _ = self.disable();
        unsafe {
            sys::dac_continuous_del_channels(self.handle);
        }
    }
}

// we write 16 bit samples to the DAC, although we only have 8 bit resolution.
// we add a 0x80 bias to i8 samples in conversion to u8, then left shift by 8
// to get the final output sample.
#[derive(Clone, Copy)]
struct DmaFrame(u16, u16);

impl Default for DmaFrame {
    fn default() -> Self {
        DmaFrame(0x8000, 0x8000)
    }
}

impl From<Frame> for DmaFrame {
    fn from(value: Frame) -> Self {
        let l = 0x80u8.wrapping_add_signed(value.0);
        let r = 0x80u8.wrapping_add_signed(value.1);

        let l = u16::from(l) << 8;
        let r = u16::from(r) << 8;

        DmaFrame(l, r)
    }
}

unsafe extern "C" fn on_convert_done(
    _handle: sys::dac_continuous_handle_t,
    event: *const sys::dac_event_data_t,
    _state: *mut c_void,
) -> bool {
    let event = &*event;

    // take mut slice ref to the output buffer:
    let output_ptr = event.buf.cast::<DmaFrame>();
    let output_len = event.buf_size / size_of::<DmaFrame>();
    let output = slice::from_raw_parts_mut(output_ptr, output_len);

    // copy from ringbuffer to output buffer, translating sample format
    // on the fly:
    let n = BUFFER.read_in_place(|slice1, slice2| {
        // copy from slice1 first:
        let n1 = cmp::min(output.len(), slice1.len());
        for i in 0..n1 {
            output[i] = slice1[i].into();
        }

        // copy from slice2, reslicing output to keep indices neat
        let output = &mut output[n1..];
        let n2 = cmp::min(output.len(), slice2.len());
        for i in 0..n2 {
            output[i] = slice2[i].into();
        }

        // return how many frames we read:
        n1 + n2
    });

    // fill any remaining buffer with silence:
    if n < output.len() {
        output[n..].fill(DmaFrame::default());
        STATS.dac_underruns.increment();
    }

    STATS.dac_frames_sent.add(n as u32);

    // notify writers that they can poll again:
    let result = WAKER.wake_from_isr();

    // return need wake flag:
    result.need_wake
}

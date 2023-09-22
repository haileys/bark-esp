//! Driver for the onboard DAC
//!
//! Only supports 8 bit output.

use core::ffi::c_void;
use core::future::poll_fn;
use core::mem::MaybeUninit;
use core::task::{Context, Poll};

use derive_more::From;
use esp_idf_sys as sys;

use crate::system::heap::{HeapBox, MallocError};
use crate::system::task::TaskWakerSet;

const DMA_BUFFER_COUNT: usize = 2;
pub const DMA_BUFFER_SIZE: usize = 2048;
pub const STREAM_BUFFER_SIZE: usize = 4*1024;

pub struct Dac {
    handle: sys::dac_continuous_handle_t,
    shared: HeapBox<IsrSharedState>,
}

struct IsrSharedState {
    buffer: StreamBuffer,
    notify: TaskWakerSet,
}

#[derive(Debug, From)]
pub struct DacError(sys::EspError);

#[derive(Debug, From)]
pub enum NewDacError {
    AllocStreamBuffer,
    AllocState(MallocError),
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

        let buffer = StreamBuffer::alloc()
            .ok_or(NewDacError::AllocStreamBuffer)?;

        let shared = HeapBox::alloc(IsrSharedState {
            buffer,
            notify: TaskWakerSet::new(),
        })?;

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
        let dac = Dac {
            handle,
            shared,
        };

        let callbacks = sys::dac_event_callbacks_t {
            on_convert_done: Some(on_convert_done),
            on_stop: None,
        };

        unsafe {
            sys::esp!(sys::dac_continuous_register_event_callback(
                dac.handle,
                &callbacks,
                HeapBox::as_borrowed_mut_ptr(&dac.shared) as *mut c_void,
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

    fn poll_write(&mut self, cx: &Context, data: &[u8]) -> Poll<usize> {
        // assert data is full frames:
        assert!(data.len() % 2 == 0);

        // get number of bytes we can write to streambuffer without blocking:
        let available = unsafe {
            sys::xStreamBufferSpacesAvailable(self.shared.buffer.handle)
        };

        // we want to copy the lesser of bytes free in the stream buffer,
        // and bytes we we have on hand:
        let nbytes = core::cmp::min(available, data.len());

        // we want to copy copy whole frames only:
        let nbytes = nbytes & !1;

        // if we can't write any bytes without blocking, add take to waker
        // set and return pending:
        if nbytes == 0 {
            self.shared.notify.add_task(cx);
            return Poll::Pending;
        }

        // otherwise do the copy:
        let ncopied = unsafe {
            sys::xStreamBufferSend(
                self.shared.buffer.handle,
                data.as_ptr().cast(),
                nbytes,
                0,
            )
        };

        // and return with the number of bytes we wrote:
        Poll::Ready(ncopied)
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

struct StreamBuffer {
    handle: sys::StreamBufferHandle_t,
}

impl StreamBuffer {
    pub fn alloc() -> Option<Self> {
        let handle = unsafe { sys::rtos_xStreamBufferCreate(STREAM_BUFFER_SIZE, 0) };
        if handle == core::ptr::null_mut() {
            return None
        };
        Some(StreamBuffer { handle })
    }
}

impl Drop for StreamBuffer {
    fn drop(&mut self) {
        unsafe {
            sys::vStreamBufferDelete(self.handle);
        }
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
    state: *mut c_void,
) -> bool {
    let event = &*event;

    let state = state as *const IsrSharedState;
    let state = &*state;

    // figure out how many bytes to copy, the smaller of the capacity of the
    // DMA buffer, and the bytes actually available in the stream buffer:
    let available_bytes = sys::xStreamBufferBytesAvailable(state.buffer.handle);
    let nbytes = core::cmp::min(event.buf_size, available_bytes);

    // round nbytes down to multiple of two to make sure we're always
    // transferring full frames, no matter what:
    let nbytes = nbytes & !1;

    // copy from streambuffer to DMA buffer:
    let mut receive_did_wake = 0;
    let ncopied = sys::xStreamBufferReceiveFromISR(
        state.buffer.handle,
        event.buf,
        nbytes,
        &mut receive_did_wake,
    );

    // notify writers that they can poll again:
    let notify_did_wake = state.notify.wake_from_isr().need_wake;

    // zero any remaining buffer we didn't fill:
    let nzero = event.buf_size - ncopied;
    let buf = event.buf as *mut u8;
    core::ptr::write_bytes(buf.add(ncopied), 0, nzero);

    (receive_did_wake != 0) || notify_did_wake
}

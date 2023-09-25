use core::alloc::Layout;
use core::cmp;
use core::convert::Infallible;
use core::ffi::c_void;
use core::future::poll_fn;
use core::mem;
use core::ptr::{self, NonNull};
use core::slice;
use core::sync::atomic::{AtomicUsize, AtomicU32, Ordering};
use core::task::{Context, Poll};

use futures::Stream;
use static_assertions::const_assert_eq;

use crate::system::heap::{MallocError, self};
use crate::system::task::TaskWakerSet;

use super::isr::IsrResult;

pub fn channel(capacity: usize) -> Result<(StreamSender, StreamReceiver), MallocError> {
    let shared = SharedRef::alloc(capacity)?;
    let sender = StreamSender { shared: shared.clone() };
    let receiver = StreamReceiver { shared };
    Ok((sender, receiver))
}

#[repr(transparent)]
pub struct StreamSender {
    shared: SharedRef,
}

impl StreamSender {
    pub async fn write(&mut self, mut data: &[u8]) {
        while data.len() > 0 {
            let bytes = poll_fn(|cx| self.poll_write(cx, data)).await;
            data = &data[bytes..];
        }
    }

    pub fn poll_write(&mut self, cx: &Context, data: &[u8]) -> Poll<usize> {
        // reader always moves forward
        let header = self.shared.header();
        let reader = header.reader.load(Ordering::Acquire);
        let writer = header.writer.load(Ordering::Acquire);

        if reader == writer {
            // buffer is full
            header.notify.add_task(cx);
            return Poll::Pending;
        }

        let capacity =
            if writer < reader {
                // simple contiguous case, no wraparound
                reader - writer
            } else {
                // write at most up to the wraparound point, a subsequent
                // call can write the next chunk from 0
                header.length - writer
            };

        let nbytes = cmp::min(capacity, data.len());

        unsafe {
            let ptr = self.shared.buffer().add(writer);
            ptr::copy(data.as_ptr(), ptr, nbytes);
        }

        let writer = (writer + nbytes) % header.length;
        header.writer.store(writer, Ordering::Release);

        return Poll::Ready(nbytes);
    }
}

impl Drop for StreamSender {
    fn drop(&mut self) {
        unsafe { self.shared.drop_tx(); }
    }
}

pub struct UnsafeStreamReceiver {
    shared: SharedRef,
}

/// This type does not implement Drop. You gotta do it yourself!
#[repr(transparent)]
pub struct RawStreamReceiver {
    shared: SharedRef,
}

#[repr(transparent)]
pub struct StreamReceiver {
    shared: SharedRef,
}

impl StreamReceiver {
    pub fn into_raw(self) -> RawStreamReceiver {
        self.shared.ptr.as_ptr().cast()
    }

    pub unsafe fn borrow_from_raw_ptr<'a>(ptr: &'a mut *mut c_void) -> &'a mut StreamReceiver {
        // SAFETY: this is safe because StreamReceiver and SharedRef are both repr(transparent) around a SharedRef
        mem::transmute(ptr)
    }

    fn read_internal(&mut self, mut out: &mut [u8]) -> usize {
        let header = self.shared.header();
        let buffer = self.shared.buffer();

        let reader = header.reader.load(Ordering::Acquire);
        let writer = header.writer.load(Ordering::Acquire);

        let mut total_bytes = 0;

        // copy from first slice
        let copy_len = cmp::min(out.len(), slices.0.len());
        out[0..copy_len].copy_from_slice(&slices.0[0..copy_len]);
        total_bytes += copy_len;
        out = &mut out[copy_len..];

        // copy from second slice
        let copy_len = cmp::min(out.len(), slices.1.len());
        out[0..copy_len].copy_from_slice(&slices.1[0..copy_len]);
        total_bytes += copy_len;

        total_bytes
    }

    pub unsafe fn read_from_isr(&mut self, out: &mut [u8]) -> IsrResult<usize, Infallible> {
        let bytes = self.read_internal(out);
        if bytes > 0 {
            self.shared.header().notify.wake_from_isr()
                .map(|()| bytes)
        } else {
            IsrResult::ok(bytes, false)
        }
    }
}

impl Drop for StreamReceiver {
    fn drop(&mut self) {
        unsafe { self.shared.drop_rx(); }
    }
}

const TX_ALIVE: u32 = 1 << 0;
const RX_ALIVE: u32 = 1 << 1;

struct Header {
    notify: TaskWakerSet,
    reader: AtomicUsize,
    writer: AtomicUsize,
    flags: AtomicU32,
    length: usize,
}

const HEADER_SIZE: usize = 20;
const HEADER_ALIGN: usize = 4;
const_assert_eq!(HEADER_SIZE, mem::size_of::<Header>());
const_assert_eq!(HEADER_ALIGN, mem::align_of::<Header>());

#[derive(Clone)]
#[repr(transparent)]
struct SharedRef {
    ptr: NonNull<Header>
}

impl SharedRef {
    fn layout(capacity: usize) -> Layout {
        Layout::from_size_align(
            HEADER_SIZE + capacity,
            HEADER_ALIGN,
        ).unwrap()
    }

    pub fn alloc(capacity: usize) -> Result<Self, MallocError> {
        let header = Header {
            notify: TaskWakerSet::new(),
            reader: AtomicUsize::new(0),
            writer: AtomicUsize::new(0),
            flags: AtomicU32::new(TX_ALIVE | RX_ALIVE),
            length: capacity,
        };

        let ptr = heap::alloc_layout(Self::layout(capacity))?.cast::<Header>();
        unsafe { ptr::write(ptr.as_ptr(), header); }

        Ok(SharedRef { ptr: ptr.cast() })
    }

    unsafe fn dealloc(&mut self) {
        let length = self.header().length;
        heap::free_layout(self.ptr.cast(), Self::layout(length));
    }

    pub fn header(&self) -> &Header {
        unsafe { self.ptr.as_ref() }
    }

    pub fn buffer(&self) -> *mut u8 {
        unsafe { self.ptr.as_ptr().cast::<u8>().add(HEADER_SIZE) }
    }

    unsafe fn drop_rx(&mut self) {
        self.drop_side(RX_ALIVE);
    }

    unsafe fn drop_tx(&mut self) {
        self.drop_side(TX_ALIVE);
    }

    unsafe fn drop_side(&mut self, flags: u32) {
        let prev = self.header().flags.fetch_and(!flags, Ordering::SeqCst);
        let current = prev & !flags;
        if current == 0 {
            self.dealloc();
        }
    }
}

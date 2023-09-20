use core::alloc::Layout;
use core::future::poll_fn;
use core::marker::PhantomData;
use core::mem::{MaybeUninit, size_of};
use core::ptr::{NonNull, self};
use core::sync::atomic::{AtomicU8, Ordering};
use core::task::{Poll, Context};

use bitflags::bitflags;
use esp_idf_sys as sys;

use crate::system::heap::{self, MallocError, HeapBox};
use crate::system::task::TaskWakerSet;

#[repr(C)]
struct Shared {
    buffer: sys::StaticStreamBuffer_t,
    flags: AtomicFlags,
    notify_rx: TaskWakerSet,
    notify_tx: TaskWakerSet,
}

pub struct StreamSender<T> {
    handle: Handle,
    _phantom: PhantomData<T>,
}

pub struct StreamReceiver<T> {
    handle: Handle,
    _phantom: PhantomData<T>,
}

#[allow(unused)]
pub fn channel<T>(capacity: usize)
    -> Result<(StreamSender<T>, StreamReceiver<T>), MallocError>
{
    let shared = HeapBox::alloc(Shared {
        buffer: sys::StaticStreamBuffer_t::default(),
        flags: AtomicFlags::new(),
        notify_rx: TaskWakerSet::new(),
        notify_tx: TaskWakerSet::new(),
    })?;

    let storage_layout = Layout::array::<T>(capacity).unwrap();

    let storage = heap::alloc_layout(storage_layout)?;

    let shared = HeapBox::into_raw(shared);

    let handle = unsafe {
        sys::xStreamBufferGenericCreateStatic(
            storage_layout.size(),
            size_of::<T>(),
            0,
            storage.cast::<u8>().as_ptr(),
            shared.cast::<sys::StaticStreamBuffer_t>().as_ptr(),
        )
    };

    assert!(handle != ptr::null_mut());

    let sender = StreamSender {
        handle: Handle(handle),
        _phantom: PhantomData,
    };

    let receiver = StreamReceiver {
        handle: Handle(handle),
        _phantom: PhantomData,
    };

    Ok((sender, receiver))
}

unsafe impl<T> Send for StreamSender<T> {}
unsafe impl<T> Send for StreamReceiver<T> {}

impl<T: Send> StreamReceiver<T> {
    pub fn poll_receive(&mut self, cx: &Context) -> Poll<T> {
        let shared = self.handle.shared();

        let bytes_for_read = unsafe {
            sys::xStreamBufferBytesAvailable(self.handle.0)
        };

        if bytes_for_read < size_of::<T>() {
            shared.notify_rx.add_task(cx);
            return Poll::Pending;
        }

        let mut value = MaybeUninit::<T>::uninit();

        let nbytes = unsafe {
            sys::xStreamBufferReceive(
                self.handle.0,
                value.as_mut_ptr().cast(),
                size_of::<T>(),
                0
            )
        };

        assert!(nbytes == size_of::<T>());

        shared.notify_tx.wake_all();

        Poll::Ready(unsafe { value.assume_init() })
    }

    pub async fn receive(&mut self) -> T {
        poll_fn(|cx| self.poll_receive(cx)).await
    }
}

impl<T: Send> StreamSender<T> {
    fn has_capacity_for_write(&self) -> bool {
        let bytes_for_write = unsafe {
            sys::xStreamBufferSpacesAvailable(self.handle.0)
        };

        bytes_for_write >= size_of::<T>()
    }

    pub fn poll_reserve(&mut self, cx: &Context) -> Poll<()> {
        if self.has_capacity_for_write() {
            Poll::Ready(())
        } else {
            self.handle.shared().notify_tx.add_task(cx);
            Poll::Pending
        }
    }

    pub fn try_send(&mut self, value: T) -> Result<(), T> {
        if !self.has_capacity_for_write() {
            // sending would write a partial object, don't try
            return Err(value);
        }

        // value transitions from being owned to unowned in this function
        let value = MaybeUninit::new(value);

        let nbytes = unsafe {
            sys::xStreamBufferSend(
                self.handle.0,
                value.as_ptr().cast(),
                size_of::<T>(),
                0,
            )
        };

        assert!(nbytes == size_of::<T>());

        self.handle.shared().notify_rx.wake_all();

        Ok(())
    }

    pub async fn send(&mut self, value: T) {
        poll_fn(|cx| self.poll_reserve(cx)).await;
        if let Err(_) = self.try_send(value) {
            panic!("streambuffer still full even after poll_reserve ready");
        }
    }
}

impl<T> Drop for StreamSender<T> {
    fn drop(&mut self) {
        unsafe { self.handle.clear_flags(Flags::TX_ALIVE); }
    }
}

impl<T> Drop for StreamReceiver<T> {
    fn drop(&mut self) {
        unsafe { self.handle.clear_flags(Flags::RX_ALIVE); }
    }
}

#[repr(transparent)]
struct Handle(sys::StreamBufferHandle_t);

impl Handle {
    unsafe fn clear_flags(&mut self, flags: Flags) {
        let prev = self.shared().flags.clear(flags, Ordering::SeqCst);
        let current = prev.difference(flags);
        if current.is_empty() {
            self.dealloc();
        }
    }

    unsafe fn dealloc(&mut self) {
        let (shared, storage) = self.get_static_ptrs();
        heap::free(shared);
        heap::free(storage);
    }

    fn shared(&self) -> &Shared {
        unsafe { self.get_static_ptrs().0.as_ref() }
    }

    fn get_static_ptrs(&self) -> (NonNull<Shared>, NonNull<u8>) {
        let mut storage_area: *mut u8 = ptr::null_mut();
        let mut buffer: *mut sys::StaticStreamBuffer_t = ptr::null_mut();

        let rc = unsafe {
            sys::xStreamBufferGetStaticBuffers(
                self.0,
                &mut storage_area,
                &mut buffer,
            )
        };

        if rc != 1 {
            panic!("xStreamBufferGetStaticBuffers failed!")
        }

        let storage_area = unsafe { NonNull::new_unchecked(storage_area) };
        let buffer = unsafe { NonNull::new_unchecked(buffer) };

        // buffer is the first field of the Shared struct, which is repr(C),
        // so it is safe to cast the buffer pointer to its containing struct:
        let buffer = buffer.cast::<Shared>();

        (buffer, storage_area)
    }
}

bitflags! {
    #[derive(Clone, Copy)]
    struct Flags: u8 {
        const RX_ALIVE = 0x01;
        const TX_ALIVE = 0x02;
    }
}

#[repr(transparent)]
struct AtomicFlags(AtomicU8);

impl AtomicFlags {
    pub fn new() -> Self {
        AtomicFlags(AtomicU8::new(Flags::all().bits()))
    }

    /// Returns previous value
    pub fn clear(&self, flags: Flags, order: Ordering) -> Flags {
        let bits = self.0.fetch_and(flags.complement().bits(), order);
        Flags::from_bits_retain(bits)
    }
}

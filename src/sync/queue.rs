use core::convert::Infallible;
use core::future::poll_fn;
use core::marker::PhantomData;
use core::mem::{MaybeUninit, size_of};
use core::ptr::NonNull;
use core::sync::atomic::{Ordering, AtomicU32};
use core::task::{Poll, Context};

use derive_more::From;
use esp_idf_sys as sys;

use crate::system::heap::{MallocError, HeapBox};
use crate::system::task::TaskWakerSet;

use super::isr::IsrResult;

const RX_ALIVE: u32 = 0x01;
const TX_ALIVE: u32 = 0x02;

struct Shared<T> {
    handle: QueueHandle<T>,
    flags: AtomicU32,
    notify_rx: TaskWakerSet,
    notify_tx: TaskWakerSet,
}

pub struct QueueSender<T> {
    shared: SharedRef<T>,
}

pub struct QueueReceiver<T> {
    shared: SharedRef<T>,
}

#[derive(Debug, From)]
pub enum AllocQueueError {
    QueueCreate,
    Malloc(MallocError),
}

#[allow(unused)]
pub fn channel<T>(capacity: usize)
    -> Result<(QueueSender<T>, QueueReceiver<T>), AllocQueueError>
{
    let handle = QueueHandle::alloc(capacity)?;

    let shared = HeapBox::alloc(Shared {
        handle,
        flags: AtomicU32::new(TX_ALIVE | RX_ALIVE),
        notify_rx: TaskWakerSet::new(),
        notify_tx: TaskWakerSet::new(),
    })?;

    let ptr = HeapBox::into_raw(shared);
    let sender = QueueSender { shared: SharedRef { ptr } };
    let receiver = QueueReceiver { shared: SharedRef { ptr } };

    Ok((sender, receiver))
}

unsafe impl<T> Send for QueueSender<T> {}
unsafe impl<T> Send for QueueReceiver<T> {}

impl<T: Send> QueueReceiver<T> {
    pub fn try_receive(&mut self) -> Option<T> {
        let shared = self.shared.as_ref();

        unsafe {
            let mut item = MaybeUninit::<T>::uninit();

            let ok = sys::rtos_queue_receive(
                shared.handle.as_ptr(),
                item.as_mut_ptr().cast(),
                0,
            );

            if ok {
                shared.notify_tx.wake_all();
                Some(item.assume_init())
            } else {
                None
            }
        }
    }

    pub fn poll_receive(&mut self, cx: &Context) -> Poll<T> {
        if let Some(item) = self.try_receive() {
            Poll::Ready(item)
        } else {
            self.shared.as_ref().notify_rx.add_task(cx);
            Poll::Pending
        }
    }

    pub async fn receive(&mut self) -> T {
        poll_fn(|cx| self.poll_receive(cx)).await
    }
}

impl<T: Send> QueueSender<T> {
    pub fn try_send(&mut self, item: T) -> Result<(), T> {
        let shared = self.shared.as_ref();
        let item = MaybeUninit::new(item);

        unsafe {
            let ok = sys::rtos_queue_send_to_back(
                shared.handle.as_ptr(),
                item.as_ptr().cast(),
                0,
            );

            if ok {
                shared.notify_rx.wake_all();
                Ok(())
            } else {
                Err(item.assume_init())
            }
        }
    }

    #[allow(unused)]
    pub unsafe fn send_from_isr(&mut self, item: T) -> IsrResult<(), T> {
        let shared = self.shared.as_ref();
        let item = MaybeUninit::new(item);
        let mut need_wake = false;

        let ok = sys::rtos_queue_send_to_back_from_isr(
            shared.handle.as_ptr(),
            item.as_ptr().cast(),
            &mut need_wake,
        );

        if ok {
            let result = shared.notify_rx.wake_from_isr();
            result.chain(IsrResult::ok((), need_wake))
        } else {
            IsrResult::err(item.assume_init(), need_wake)
        }
    }

    #[allow(unused)]
    pub unsafe fn send_overwriting_from_isr(&mut self, item: T) -> IsrResult<(), Infallible> {
        let shared = self.shared.as_ref();
        let item = MaybeUninit::new(item);
        let mut need_wake_receive = false;
        let mut need_wake_send_to_back = false;

        // pop an item from the front of the queue if it's full:
        let full = sys::xQueueIsQueueFullFromISR(shared.handle.as_ptr());
        if full != 0 {
            // panic!("queue full for whatever reason??");
            // pop item at front
            let mut dummy = MaybeUninit::uninit();
            let rc = sys::xQueueReceiveFromISR(
                shared.handle.as_ptr(),
                dummy.as_mut_ptr(),
                &mut need_wake_receive as *mut bool as *mut i32,
            );
            if rc != 0 {
                // make sure its dropped
                dummy.assume_init();
            }
        }

        sys::rtos_queue_send_to_back_from_isr(
            shared.handle.as_ptr(),
            item.as_ptr().cast(),
            &mut need_wake_send_to_back,
        );

        let result = shared.notify_rx.wake_from_isr();
        result.chain(IsrResult::ok((), need_wake_receive || need_wake_send_to_back))
    }

    pub fn available(&self) -> usize {
        let shared = self.shared.as_ref();
        unsafe { sys::uxQueueSpacesAvailable(shared.handle.as_ptr()) as usize }
    }

    pub fn poll_reserve(&mut self, cx: &Context) -> Poll<()> {
        if self.available() > 0 {
            Poll::Ready(())
        } else {
            let shared = self.shared.as_ref();
            shared.notify_tx.add_task(cx);
            Poll::Pending
        }
    }

    #[allow(unused)]
    pub async fn send(&mut self, value: T) {
        poll_fn(|cx| self.poll_reserve(cx)).await;
        if let Err(_) = self.try_send(value) {
            panic!("queue still full even after poll_reserve ready");
        }
    }
}

impl<T> Drop for QueueSender<T> {
    fn drop(&mut self) {
        unsafe { self.shared.drop_tx(); }
    }
}

impl<T> Drop for QueueReceiver<T> {
    fn drop(&mut self) {
        unsafe { self.shared.drop_rx(); }
    }
}

struct SharedRef<T> {
    ptr: NonNull<Shared<T>>
}

impl<T> SharedRef<T> {
    fn as_ref(&self) -> &Shared<T> {
        unsafe { self.ptr.as_ref() }
    }

    unsafe fn drop_rx(&mut self) {
        self.drop_side(RX_ALIVE);
    }

    unsafe fn drop_tx(&mut self) {
        self.drop_side(TX_ALIVE);
    }

    unsafe fn drop_side(&mut self, flags: u32) {
        let prev = self.as_ref().flags.fetch_and(!flags, Ordering::SeqCst);
        let current = prev & !flags;
        if current == 0 {
            // return ptr back to HeapBox to drop:
            HeapBox::from_raw(self.ptr);
        }
    }
}

#[repr(transparent)]
struct QueueHandle<T> {
    ptr: NonNull<sys::QueueDefinition>,
    _phantom: PhantomData<T>,
}

impl<T> QueueHandle<T> {
    pub fn alloc(capacity: usize) -> Result<Self, AllocQueueError> {
        let ptr = unsafe { sys::rtos_queue_create(capacity, size_of::<T>()) };
        let ptr = NonNull::new(ptr).ok_or(AllocQueueError::QueueCreate)?;
        Ok(QueueHandle { ptr, _phantom: PhantomData })
    }

    fn as_ptr(&self) -> sys::QueueHandle_t {
        self.ptr.as_ptr()
    }
}

impl<T> Drop for QueueHandle<T> {
    fn drop(&mut self) {
        // first drop all remaining items in queue
        loop {
            let mut item = MaybeUninit::<T>::uninit();

            let received_item = unsafe {
                sys::rtos_queue_receive(
                    self.as_ptr(),
                    item.as_mut_ptr().cast(),
                    0,
                )
            };

            if received_item {
                // assume_init the item and then drop it
                unsafe { item.assume_init(); }
            } else {
                break;
            }
        }

        // then delete the queue
        unsafe { sys::rtos_queue_delete(self.as_ptr()); }
    }
}

use core::convert::Infallible;
use core::sync::atomic::{Ordering, AtomicU32};
use core::ptr::{null_mut, NonNull};
use core::task::{Context, Waker};

use esp_idf_sys as sys;

use crate::sync::isr::IsrResult;

use super::TaskPtr;
use super::registry::{self, TaskId};

pub struct TaskWaker {
    id: TaskId,
}

impl TaskWaker {
    pub fn new(id: TaskId) -> Self {
        TaskWaker { id }
    }

    pub fn to_waker(&self) -> Waker {
        unsafe { Waker::from_raw(waker_impl::new(self.id)) }
    }

    pub fn from_context(cx: &Context) -> Self {
        let id = waker_impl::task_id(cx.waker().as_raw())
            .unwrap_or_else(|| {
                // we wake an entire freertos task at once, so if we can't
                // cheaply retrieve the task id from the waker, we can scan
                // the registry:
                TaskId::current()
            });

        TaskWaker { id }
    }
}

pub struct TaskWakerSet {
    bits: AtomicU32,
}

impl TaskWakerSet {
    pub fn new() -> Self {
        TaskWakerSet { bits: AtomicU32::new(0) }
    }

    pub fn add_task(&self, context: &Context) {
        let waker = TaskWaker::from_context(context);
        let task_bit = waker.id.as_bit();
        self.bits.fetch_or(task_bit, Ordering::SeqCst);
    }

    pub fn wake_all(&self) {
        for task in self.take_wakeable() {
            unsafe { wake(task); }
        }
    }

    pub unsafe fn wake_from_isr(&self) -> IsrResult<(), Infallible> {
        let mut result = IsrResult::default();

        for task in self.take_wakeable() {
            result = result.chain(wake_from_isr(task));
        }

        result
    }

    fn take_wakeable(&self) -> impl Iterator<Item = TaskPtr> + 'static {
        let bitset = self.bits.swap(0, Ordering::SeqCst);

        TaskId::iter()
            .filter(move |id| (bitset & id.as_bit()) != 0)
            .filter_map(|id| id.slot().load())
    }
}

unsafe fn wake(ptr: NonNull<sys::tskTaskControlBlock>) {
    sys::xTaskGenericNotify(
        ptr.as_ptr(),
        0,
        0,
        sys::eNotifyAction_eNoAction,
        null_mut(),
    );
}

unsafe fn wake_from_isr(ptr: NonNull<sys::tskTaskControlBlock>) -> IsrResult<(), Infallible> {
    let mut need_wake = 0;
    sys::xTaskGenericNotifyFromISR(
        ptr.as_ptr(),
        0,
        0,
        sys::eNotifyAction_eNoAction,
        null_mut(),
        &mut need_wake,
    );

    IsrResult::ok((), need_wake != 0)
}

mod waker_impl {
    use core::task::{RawWaker, RawWakerVTable};
    use super::registry::TaskId;

    static VTABLE: RawWakerVTable = RawWakerVTable::new(
        clone,
        wake,
        wake,
        drop,
    );

    pub fn new(id: TaskId) -> RawWaker {
        RawWaker::new(id.as_opaque_ptr(), &VTABLE)
    }

    fn same_vtable(waker: &RawWaker) -> bool {
        let waker_vtable_ptr = waker.vtable() as *const _;
        let our_vtable_ptr = &VTABLE as *const _;
        waker_vtable_ptr == our_vtable_ptr
    }

    pub fn task_id(waker: &RawWaker) -> Option<TaskId> {
        if same_vtable(waker) {
            Some(TaskId::from_opaque_ptr(waker.data()))
        } else {
            None
        }
    }

    unsafe fn clone(data: *const ()) -> RawWaker {
        RawWaker::new(data, &VTABLE)
    }

    unsafe fn wake(data: *const ()) {
        let id = TaskId::from_opaque_ptr(data);

        if let Some(task) = id.slot().load() {
            super::wake(task);
        }
    }

    unsafe fn drop(_: *const ()) {
        // nothing to do here
        // we don't even need to keep track of wakers holding set bits for tasks
        // which have since finished, because spurious wakeups are safe!
    }
}

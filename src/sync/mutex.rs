use core::{cell::UnsafeCell, ops::{DerefMut, Deref}, sync::atomic::{AtomicBool, Ordering}, task::{Context, Poll}, future::poll_fn};

use esp_idf_sys as sys;

use crate::system::task::TaskWakerSet;

pub struct CriticalMutex<T> {
    spinlock: sys::portMUX_TYPE,
    inner: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for CriticalMutex<T> {}

#[allow(unused)]
impl<T> CriticalMutex<T> {
    pub fn new(value: T) -> Self {
        CriticalMutex {
            spinlock: Default::default(),
            inner: UnsafeCell::new(value),
        }
    }

    pub fn lock(&self) -> CriticalMutexGuard<'_, T> {
        unsafe { sys::rtos_taskENTER_CRITICAL(&self.spinlock); }
        CriticalMutexGuard { mutex: self }
    }
}

pub struct CriticalMutexGuard<'a, T> {
    mutex: &'a CriticalMutex<T>
}

impl<'a, T> Deref for CriticalMutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: we're in a critical section
        unsafe { &*self.mutex.inner.get() }
    }
}

impl<'a, T> DerefMut for CriticalMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: we're in a critical section
        unsafe { &mut *self.mutex.inner.get() }
    }
}

impl<'a, T> Drop for CriticalMutexGuard<'a, T> {
    fn drop(&mut self) {
        unsafe { sys::rtos_taskEXIT_CRITICAL(&self.mutex.spinlock); }
    }
}

pub struct TaskMutex<T> {
    flag: AtomicBool,
    notify: TaskWakerSet,
    inner: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for TaskMutex<T> {}

impl<T> TaskMutex<T> {
    pub fn new(value: T) -> Self {
        TaskMutex {
            flag: AtomicBool::new(false),
            notify: TaskWakerSet::new(),
            inner: UnsafeCell::new(value),
        }
    }

    fn poll_lock(&self, cx: &Context) -> Poll<TaskMutexGuard<'_, T>> {
        if self.flag.swap(true, Ordering::SeqCst) {
            self.notify.add_task(cx);
            Poll::Pending
        } else {
            Poll::Ready(TaskMutexGuard { mutex: self })
        }
    }

    pub async fn lock(&self) -> TaskMutexGuard<'_, T> {
        poll_fn(|cx| self.poll_lock(cx)).await
    }
}

pub struct TaskMutexGuard<'a, T> {
    mutex: &'a TaskMutex<T>
}

impl<'a, T> Deref for TaskMutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: we're in a critical section
        unsafe { &*self.mutex.inner.get() }
    }
}

impl<'a, T> DerefMut for TaskMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: we're in a critical section
        unsafe { &mut *self.mutex.inner.get() }
    }
}

impl<'a, T> Drop for TaskMutexGuard<'a, T> {
    fn drop(&mut self) {
        self.mutex.flag.store(false, Ordering::SeqCst);
        self.mutex.notify.wake_all();
    }
}

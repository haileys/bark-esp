use core::{cell::UnsafeCell, ops::{DerefMut, Deref}};

use esp_idf_sys as sys;

pub struct Mutex<T> {
    spinlock: sys::portMUX_TYPE,
    inner: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for Mutex<T> {}

impl<T> Mutex<T> {
    pub fn new(value: T) -> Self {
        Mutex {
            spinlock: Default::default(),
            inner: UnsafeCell::new(value),
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, T> {
        unsafe { sys::rtos_taskENTER_CRITICAL(&self.spinlock); }
        MutexGuard { mutex: self }
    }
}

pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>
}

impl<'a, T> Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: we're in a critical section
        unsafe { &*self.mutex.inner.get() }
    }
}

impl<'a, T> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: we're in a critical section
        unsafe { &mut *self.mutex.inner.get() }
    }
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        unsafe { sys::rtos_taskEXIT_CRITICAL(&self.mutex.spinlock); }
    }
}

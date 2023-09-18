use core::{cell::SyncUnsafeCell, ops::{DerefMut, Deref}};

use esp_idf_sys as sys;

// pub mod raw;

pub struct OsMutex<T> {
    value: SyncUnsafeCell<T>,
    sema: sys::SemaphoreHandle_t,
}

#[derive(Debug)]
pub struct AllocMutexError;

impl<T> OsMutex<T> {
    #[allow(unused)]
    pub fn new(value: T) -> Result<Self, AllocMutexError> {
        let sema = unsafe { sys::bark_create_recursive_mutex() };

        if sema == core::ptr::null_mut() {
            return Err(AllocMutexError);
        }

        Ok(OsMutex {
            value: SyncUnsafeCell::new(value),
            sema,
        })
    }

    #[allow(unused)]
    pub fn lock(&self) -> MutexGuard<'_, T> {
        unsafe { sys::bark_lock_recursive_mutex(self.sema); }
        MutexGuard { mutex: self }
    }
}

impl<T> Drop for OsMutex<T> {
    fn drop(&mut self) {
        unsafe { sys::bark_delete_recursive_mutex(self.sema); }
    }
}

pub struct MutexGuard<'a, T> {
    mutex: &'a OsMutex<T>,
}

impl<'a, T> Deref for MutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        let ptr = self.mutex.value.get() as *const T;
        // SAFETY: we hold the mutex
        unsafe { &*ptr }
    }
}

impl<'a, T> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        let ptr = self.mutex.value.get();
        // SAFETY: we hold the mutex
        unsafe { &mut *ptr }
    }
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        unsafe { sys::bark_unlock_recursive_mutex(self.mutex.sema); }
    }
}

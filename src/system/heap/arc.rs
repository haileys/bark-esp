use core::ptr::NonNull;
use core::sync::atomic::{Ordering, AtomicUsize};
use core::ops::Deref;

use super::{HeapBox, MallocError};

pub struct SharedBox<T> {
    ptr: NonNull<Inner<T>>
}

unsafe impl<T: Send + Sync> Send for SharedBox<T> {}
unsafe impl<T: Send + Sync> Sync for SharedBox<T> {}

struct Inner<T> {
    data: T,
    refcount: AtomicUsize,
}

impl<T> SharedBox<T> {
    pub fn alloc(value: T) -> Result<Self, MallocError> {
        let ptr = HeapBox::into_raw(HeapBox::alloc(Inner {
            data: value,
            refcount: AtomicUsize::new(1),
        })?);

        Ok(SharedBox { ptr })
    }

    pub fn unique(shared: &SharedBox<T>) -> bool {
        let shared = unsafe { shared.ptr.as_ref() };
        shared.refcount.load(Ordering::Relaxed) == 1
    }
}

impl<T> Deref for SharedBox<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &self.ptr.as_ref().data }
    }
}

impl<T> Clone for SharedBox<T> {
    fn clone(&self) -> Self {
        unsafe {
            self.ptr.as_ref().refcount.fetch_add(1, Ordering::SeqCst);
        }
        SharedBox { ptr: self.ptr }
    }
}

impl<T> Drop for SharedBox<T> {
    fn drop(&mut self) {
        let refcount = unsafe {
            self.ptr.as_ref().refcount.fetch_sub(1, Ordering::SeqCst)
        };

        if refcount == 1 {
            // if previous refcount before fetch_sub was 1, we were the only
            // owner. drop
            unsafe { HeapBox::from_raw(self.ptr); }
        }
    }
}

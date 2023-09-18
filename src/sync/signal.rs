use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Waker, Context, Poll};

use esp_idf_sys as sys;
use futures::Future;

use crate::system::heap::{HeapBox, MallocError};

pub unsafe fn init() {
    sys::bark_sync_signal_init();
}

pub struct Signal<T> {
    inner: UnsafeCell<Inner<T>>,
}

unsafe impl<T> Sync for Signal<T> {}

impl<T: Copy + PartialEq> Signal<T> {
    pub const fn new(init: T) -> Self {
        Signal {
            inner: UnsafeCell::new(Inner {
                value: init,
                watches: WatchList::new(),
            })
        }
    }
}

struct Inner<T> {
    value: T,
    watches: WatchList,
}

impl<T: Copy + PartialEq> Signal<T> {
    pub fn set(&self, value: T) {
        let _lock = lock();

        // SAFETY: we just took global lock
        let inner = unsafe { &mut *self.inner.get() };

        inner.value = value;
        inner.watches.wake();
    }

    pub fn watch(&self) -> Result<Watch<T>, MallocError> {
        let _lock = lock();

        // SAFETY: we just took global lock
        let inner = unsafe { &mut *self.inner.get() };

        Ok(Watch {
            signal: self,
            node: inner.watches.add()?,
            current: None,
        })
    }
}

pub struct Watch<'s, T> {
    signal: &'s Signal<T>,
    node: WatchNodeRef<'s>,
    current: Option<T>,
}

impl<'s, T: Copy + PartialEq> Watch<'s, T> {
    pub fn wait<'w>(&'w mut self) -> WatchFuture<'s, 'w, T> {
        WatchFuture { watch: self }
    }
}

pub struct WatchFuture<'s, 'w, T> {
    watch: &'w mut Watch<'s, T>,
}

impl<'s, 'w, T: Copy + PartialEq> Future for WatchFuture<'s, 'w, T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<T>
    {
        let this = self.get_mut();

        let _lock = lock();

        // SAFETY: we hold global lock
        let inner = unsafe { &mut *this.watch.signal.inner.get() };

        if this.watch.current != Some(inner.value) {
            // remove waker if there is one
            // SAFETY: we hold global lock
            let node = unsafe { &mut *this.watch.node.ptr };
            node.waker = None;

            // set current and return new value
            let value = inner.value;
            this.watch.current = Some(value);
            return Poll::Ready(value);
        }

        // otherwise set waker and return pending
        // SAFETY: we hold global lock
        let node = unsafe { &mut *this.watch.node.ptr };
        node.waker = Some(cx.waker().clone());

        Poll::Pending
    }
}

struct WatchList {
    watches: Option<HeapBox<WatchNode>>,
}

struct WatchNode {
    waker: Option<Waker>,
    alive: bool,
    next: Option<HeapBox<WatchNode>>,
}

struct WatchNodeRef<'a> {
    ptr: *mut WatchNode,
    _phantom: PhantomData<&'a WatchNode>,
}

impl<'a> Drop for WatchNodeRef<'a> {
    fn drop(&mut self) {
        let _lock = lock();

        // SAFETY: we hold global lock
        let node = unsafe { &mut *self.ptr };

        node.alive = false;
        node.waker = None;
    }
}

impl WatchList {
    pub const fn new() -> Self {
        WatchList { watches: None }
    }

    pub fn add<'a>(&'a mut self) -> Result<WatchNodeRef<'a>, MallocError> {
        let node = HeapBox::alloc(WatchNode {
            waker: None,
            alive: true,
            next: self.watches.take(),
        })?;

        let node_ref = WatchNodeRef {
            ptr: node.as_mut_ptr(),
            _phantom: PhantomData,
        };

        self.watches = Some(node);

        Ok(node_ref)
    }

    pub fn wake(&mut self) {
        // clear all dead watches first:
        let slot = &mut self.watches;
        while let Some(node) = slot {
            if !node.as_ref().alive {
                let node_next = node.as_mut().next.take();
                *slot = node_next;
            }
        }

        // then wake all remaining:
        let mut next = self.watches.as_mut();
        while let Some(node) = next {
            let node = node.as_mut();
            if let Some(waker) = node.waker.take() {
                waker.wake();
            }
            next = node.next.as_mut();
        }
    }
}

// the big global lock for this module
fn lock() -> impl Drop {
    struct TheLock;

    impl Drop for TheLock {
        fn drop(&mut self) {
            unsafe { sys::bark_sync_signal_unlock(); }
        }
    }

    unsafe { sys::bark_sync_signal_lock(); }
    TheLock
}

use core::alloc::Layout;
use core::marker::PhantomData;
use core::mem::{MaybeUninit, size_of};
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU8, Ordering};

use crate::util::flexi::FlexiPtr;

use super::heap::{self, MallocError};

pub fn new<T>(item_capacity: u8) {

}

pub struct QueueSender<T> {
    ptr: HeapPtr<T>,
}

impl<T> Drop for QueueSender<T> {
    fn drop(&mut self) {
        unsafe { self.ptr.unset_flag(FLAG_TX_ALIVE); }
    }
}

pub struct QueueReceiver<T> {
    ptr: HeapPtr<T>,
}

impl<T> Drop for QueueReceiver<T> {
    fn drop(&mut self) {
        unsafe { self.ptr.unset_flag(FLAG_RX_ALIVE); }
    }
}

#[repr(C)]
struct HeapHeader {
    flags: AtomicU8,
    rx_head: AtomicU8,
    tx_head: AtomicU8,
    size: u8,
}

const FLAG_RX_ALIVE: u8 = 1 << 0;
const FLAG_TX_ALIVE: u8 = 1 << 1;

#[repr(transparent)]
struct HeapPtr<T: Sized>(FlexiPtr<HeapHeader, T>);

impl<T: Sized> HeapPtr<T> {
    fn alloc(size: u8) -> Result<Self, MallocError> {
        let flexi = FlexiPtr::alloc(size.into())?;
        unsafe {
            core::ptr::write(flexi.header_mut_ptr(), HeapHeader {
                flags: AtomicU8::new(FLAG_RX_ALIVE | FLAG_TX_ALIVE),
                rx_head: AtomicU8::new(0),
                tx_head: AtomicU8::new(0),
                size: size,
            });
        }
        Ok(HeapPtr(flexi))
    }

    unsafe fn unset_flag(&self, flag: u8) {
        let mask = !flag;
        let prev = self.header().flags.fetch_and(mask, Ordering::SeqCst);
        if (prev & mask) == 0 {
            // drop it!
            self.free();
        }
    }

    unsafe fn free(&self) {
        self.0.free()
    }

    fn header(&self) -> &HeapHeader {
        // SAFETY: header is always immutable once created
        unsafe { &*self.0.header_ptr() }
    }

    fn slot_mut_ptr(&self, idx: u8) -> *mut T {
        self.0.element_ptr_mut(idx.into())
    }
}

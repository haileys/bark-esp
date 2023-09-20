use core::sync::atomic::{AtomicU32, Ordering};

use crate::system::heap::{RawHeapArray, MallocError};

pub struct RingBuffer<T> {
    write_head: AtomicU32,
    read_head: AtomicU32,
    array: RawHeapArray<T>,
}

impl<T> RingBuffer<T> {
    pub fn new(capacity: usize) -> Result<Self, MallocError> {
        Ok(RingBuffer {
            write_head: AtomicU32::new(0),
            read_head: AtomicU32::new(0),
            array: RawHeapArray::alloc(capacity)?,
        })
    }

    unsafe fn drop_at(&mut self, idx: usize) {
        let mut item = self.array[idx];
        core::ptr::drop_in_place(item.as_mut_ptr());
    }
}

impl<T> Drop for RingBuffer<T> {
    fn drop(&mut self) {
        let read_head = self.read_head.load(Ordering::Relaxed);
        let write_head = self.write_head.load(Ordering::Relaxed);

        unsafe {
            if read_head < write_head {
                for idx in read_head..write_head {
                    self.drop_at(idx);
                }
            } else {
                for idx in read_head..self.array.len() {
                    self.drop_at(idx);
                }
                for idx in 0..write_head {
                    self.drop_at(idx);
                }
            }
        }
    }
}

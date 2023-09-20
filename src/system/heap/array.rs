use core::alloc::Layout;
use core::mem::MaybeUninit;
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;

use super::{MallocError, alloc_layout, free_layout};

/// A raw array that lives on the heap. Doesn't implement drop! Be careful!
pub struct RawHeapArray<T> {
    ptr: NonNull<MaybeUninit<T>>,
    len: usize,
}

impl<T> RawHeapArray<T> {
    fn layout(len: usize) -> Layout {
        // TODO when does this fail?
        Layout::array::<T>(len).unwrap()
    }

    pub fn alloc(len: usize) -> Result<Self, MallocError> {
        let ptr = alloc_layout(Self::layout(len))?.cast();
        Ok(RawHeapArray { ptr, len })
    }

    /// Deallocates backing storage but does not drop contents
    /// (we don't - can't - know which items are initialized or not,
    /// that's for users of this type to know)
    pub unsafe fn dealloc(&mut self) {
        unsafe { free_layout(self.ptr.cast(), Self::layout(self.len)); }
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

impl<T> Deref for RawHeapArray<T> {
    type Target = [MaybeUninit<T>];

    fn deref(&self) -> &[MaybeUninit<T>] {
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<T> DerefMut for RawHeapArray<T> {
    fn deref_mut(&mut self) -> &mut [MaybeUninit<T>] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

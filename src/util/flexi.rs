use core::alloc::Layout;
use core::marker::PhantomData;
use core::ptr::NonNull;

use crate::system::heap::{self, MallocError};

#[derive(Clone, Copy)]
pub struct FlexiPtr<Header, T>(NonNull<Header>, PhantomData<T>);

impl<Header, T> FlexiPtr<Header, T> {
    pub fn alloc(elements: usize) -> Result<Self, MallocError> {
        let ptr = unsafe { heap::alloc_layout(Self::layout(elements))? };
        Ok(FlexiPtr(ptr, PhantomData))
    }

    pub unsafe fn free(self) {
        heap::free(self.0.as_ptr())
    }

    pub fn layout(elements: usize) -> Layout {
        let header = Layout::<Header>::new();
        let elements = Layout::<T>::array(elements);
        let (layout, _offset) = header.extend(elements)
            .expect("error in flexi layout calculation");
        layout.pad_to_align()
    }

    pub fn header_ptr(&self) -> *const Header {
        self.0 as *const Header
    }

    pub fn header_mut_ptr(&self) -> *mut Header {
        self.0
    }

    pub fn element_ptr(&self, index: usize) -> *const T {
        self.element_ptr_mut(index) as *const T
    }

    pub fn element_ptr_mut(&self, index: usize) -> *const T {
        let offset = self.layout(index).size();
        unsafe { self.0.byte_add(offset).cast::<T>() }
    }
}

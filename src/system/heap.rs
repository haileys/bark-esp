use core::ffi::c_void;
use core::mem;
use core::ptr::NonNull;
use esp_idf_sys as sys;

#[derive(Debug)]
pub struct MallocError { #[allow(unused)] bytes: usize }

pub fn alloc<T>() -> Result<NonNull<T>, MallocError> {
    let bytes = mem::size_of::<T>();

    let ptr = bytes.try_into()
        .map(|bytes| unsafe { sys::malloc(bytes) as *mut T })
        .unwrap_or(core::ptr::null_mut());

    NonNull::new(ptr).ok_or(MallocError { bytes })
}

pub unsafe fn free<T>(ptr: *mut T) {
    unsafe { sys::free(ptr as *mut c_void); }
}

#[repr(transparent)]
pub struct HeapBox<T> {
    ptr: NonNull<T>,
}

impl<T> HeapBox<T> {
    pub fn alloc(value: T) -> Result<Self, MallocError> {
        let ptr = alloc::<T>()?;
        unsafe { core::ptr::write(ptr.as_ptr(), value); }
        Ok(HeapBox { ptr })
    }

    pub fn as_mut_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }

    pub fn erase_type(self) -> UntypedHeapBox {
        UntypedHeapBox {
            ptr: self.ptr.cast(),
            drop: core::ptr::drop_in_place,
        }
    }
}

impl<T> Drop for HeapBox<T> {
    fn drop(&mut self) {
        unsafe {
            core::ptr::drop_in_place(self.ptr.as_ptr());
            free(self.ptr.as_ptr());
        }
    }
}

pub struct UntypedHeapBox {
    ptr: NonNull<()>,
    drop: unsafe fn(*mut ()),
}

impl Drop for UntypedHeapBox {
    fn drop(&mut self) {
        unsafe {
            (self.drop)(self.ptr.as_ptr());
        }
    }
}

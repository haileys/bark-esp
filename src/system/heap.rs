use core::ffi::c_void;
use core::mem;
use core::ptr::NonNull;
use esp_idf_sys as sys;

#[derive(Debug)]
pub struct MallocError {
    #[allow(unused)]
    bytes: usize,
}

pub fn alloc<T>() -> Result<NonNull<T>, MallocError> {
    let bytes = mem::size_of::<T>();

    bytes.try_into()
        .map(|bytes| match bytes {
            0 => Some(setinel()),
            _ => NonNull::new(unsafe {
                sys::malloc(bytes) as *mut T
            }),
        })
        .unwrap_or_default()
        .ok_or(MallocError { bytes })
}

pub unsafe fn free<T>(ptr: *mut T) {
    unsafe { sys::free(ptr as *mut c_void); }
}

fn setinel<T>() -> NonNull<T> {
    unsafe {
        static mut SENTINEL: u8 = 0;
        NonNull::new_unchecked(&mut SENTINEL).cast()
    }
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
        type TypedDrop<T> = unsafe fn(NonNull<T>);
        type UntypedDrop = unsafe fn(NonNull<()>);

        let drop = unsafe {
            mem::transmute::<TypedDrop<T>, UntypedDrop>(drop_free::<T>)
        };

        UntypedHeapBox {
            ptr: self.ptr.cast(),
            drop,
        }
    }
}

unsafe fn drop_free<T>(ptr: NonNull<T>) {
    if ptr != setinel() {
        core::ptr::drop_in_place(ptr.as_ptr());
        free(ptr.as_ptr());
    }
}

impl<T> Drop for HeapBox<T> {
    fn drop(&mut self) {
        unsafe { drop_free(self.ptr) }
    }
}

pub struct UntypedHeapBox {
    ptr: NonNull<()>,
    drop: unsafe fn(NonNull<()>),
}

impl Drop for UntypedHeapBox {
    fn drop(&mut self) {
        unsafe {
            (self.drop)(self.ptr);
        }
    }
}

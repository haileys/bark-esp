use core::alloc::Layout;
use core::ffi::c_void;
use core::mem;
use core::ptr::NonNull;
use esp_idf_sys as sys;

#[derive(Debug)]
pub struct MallocError {
    #[allow(unused)]
    bytes: usize,
}

/// Allocates uninitialized memory to fit a `T`
pub fn alloc<T>() -> Result<NonNull<T>, MallocError> {
    unsafe {
        alloc_layout(Layout::new::<T>())
    }
}

pub unsafe fn alloc_layout<T>(layout: Layout) -> Result<NonNull<T>, MallocError> {
    let bytes = layout.size();

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

/// Frees underlying allocation without calling [`Drop::drop`] on `ptr`
pub unsafe fn free<T>(ptr: NonNull<T>) {
    if ptr != setinel() {
        unsafe { sys::free(ptr.cast::<c_void>().as_ptr()); }
    }
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

    pub fn as_ref(&self) -> &T {
        unsafe { self.ptr.as_ref() }
    }

    pub fn as_mut(&mut self) -> &mut T {
        unsafe { self.ptr.as_mut() }
    }

    /// Move ownership into a raw pointer
    pub fn into_raw(self) -> NonNull<T> {
        let ptr = self.ptr;
        // don't drop self:
        mem::forget(self);
        ptr
    }

    /// Take ownership of a raw pointer
    pub unsafe fn from_raw(ptr: NonNull<T>) -> Self {
        HeapBox { ptr }
    }

    pub fn into_inner(self) -> T {
        let inner_ptr = self.into_raw();

        // read value
        let inner = unsafe { core::ptr::read(inner_ptr.as_ptr()) };

        // release the underlying allocation manually without calling drop
        unsafe { free(inner_ptr); }

        inner
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
        free(ptr);
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

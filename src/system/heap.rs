use core::alloc::{GlobalAlloc, Layout};
use core::ffi::c_void;
use core::ptr::{NonNull, self};
use esp_idf_sys as sys;

pub mod boxed;
pub use boxed::{HeapBox, UntypedHeapBox};

#[derive(Debug)]
pub struct MallocError {
    #[allow(unused)]
    bytes: usize,
}

/// Allocates uninitialized memory to fit a `T`
pub fn alloc<T>() -> Result<NonNull<T>, MallocError> {
    let layout = Layout::new::<T>();
    NonNull::new(unsafe { SYSTEM_MALLOC.alloc(layout) })
        .map(NonNull::cast)
        .ok_or(MallocError { bytes: layout.size() })
}

/// Frees underlying allocation without calling [`Drop::drop`] on `ptr`
pub unsafe fn free<T>(ptr: NonNull<T>) {
    let layout = Layout::new::<T>();
    unsafe { SYSTEM_MALLOC.dealloc(ptr.cast().as_ptr(), layout); }
}

static SYSTEM_MALLOC: SystemMalloc = SystemMalloc;

struct SystemMalloc;

unsafe impl GlobalAlloc for SystemMalloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // ISO C spec says that malloc always returns a memory address that's
        // suitable for a pointer to any object that fits within the size
        // specified. In practise, this means we can safely ignore the align
        // requested, because we will always satisfy it anyway.

        u32::try_from(layout.size())
            .map(|bytes| match bytes {
                0 => setinel().as_ptr(),
                _ => sys::malloc(bytes),
            })
            .unwrap_or(ptr::null_mut())
            .cast::<u8>()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if layout.size() == 0 {
            // treated specially in alloc
            return;
        }

        sys::free(ptr.cast::<c_void>())
    }
}

fn setinel<T>() -> NonNull<T> {
    unsafe {
        static mut SENTINEL: u8 = 0;
        NonNull::new_unchecked(&mut SENTINEL).cast()
    }
}

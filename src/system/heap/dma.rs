use core::mem::MaybeUninit;
use core::ptr::{self, NonNull};
use core::slice;

use esp_idf_sys as sys;

pub struct DmaBuffer {
    ptr: NonNull<u8>,
    len: usize,
}

pub struct DmaBufferUninit {
    buffer: DmaBuffer,
}

#[derive(Debug, Copy, Clone)]
pub struct DmaAllocError {
    #[allow(unused)]
    bytes: usize,
}

pub fn alloc(size: usize) -> Result<DmaBufferUninit, DmaAllocError> {
    let ptr = unsafe { sys::heap_caps_malloc(size, sys::MALLOC_CAP_DMA) };
    let ptr = NonNull::new(ptr).cast().ok_or(DmaAllocError { bytes: size })?;
    let buffer = DmaBuffer { ptr, len: size };
    Ok(DmaBufferUninit { buffer })
}

impl DmaBuffer {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    pub fn bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.as_ptr().cast(), self.len()) }
    }

    pub fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr().cast(), self.len()) }
    }
}

impl DmaBufferUninit {
    pub fn len(&self) -> usize {
        self.buffer.len
    }

    pub fn as_mut_ptr(&self) -> *mut u8 {
        self.buffer.ptr.as_ptr()
    }

    pub fn bytes_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr().cast(), self.len()) }
    }

    pub fn zeroed(mut self) -> DmaBuffer {
        unsafe {
            ptr::write_bytes(self.as_mut_ptr(), 0, self.len());
            self.assume_init()
        }
    }

    pub unsafe fn assume_init(self) -> DmaBuffer {
        self.buffer
    }
}

impl Drop for DmaBuffer {
    fn drop(&mut self) {
        unsafe { sys::heap_caps_free(self.ptr.as_ptr().cast()); }
    }
}

use core::cell::UnsafeCell;
use core::cmp;
use core::slice;
use core::sync::atomic::{AtomicUsize, Ordering};

pub struct RingBuffer<T, const N: usize> {
    reader: AtomicUsize,
    writer: AtomicUsize,
    buffer: UnsafeCell<[T; N]>,
}

unsafe impl<T: Copy, const N: usize> Sync for RingBuffer<T, N> {}

impl<const N: usize> RingBuffer<u8, N> {
    pub const fn new() -> Self {
        RingBuffer {
            reader: AtomicUsize::new(0),
            writer: AtomicUsize::new(0),
            buffer: UnsafeCell::new([0u8; N]),
        }
    }
}

impl<T: Copy, const N: usize> RingBuffer<T, N> {
    /// SAFETY must not be called while there are any readers or writers
    pub unsafe fn reset(&self) {
        self.reader.store(0, Ordering::Relaxed);
        self.writer.store(0, Ordering::Relaxed);
    }

    fn buffer_ptr(&self) -> *mut T {
        self.buffer.get().cast()
    }

    fn buffer_len(&self) -> usize {
        N
    }

    /// SAFETY: only one task may be reading at any given time
    pub unsafe fn read(&self, data: &mut [T]) -> usize {
        let reader = self.reader.load(Ordering::Acquire);
        let writer = self.writer.load(Ordering::Acquire);

        let (left, right) = slices(
            self.buffer_ptr(),
            self.buffer_len(),
            reader,
            writer,
        );

        let copied = copy_from_split(left, right, data);

        let reader = (reader + copied) % N;
        self.reader.store(reader, Ordering::Release);

        copied
    }

    /// SAFETY: only one task may be writing at any given time
    pub unsafe fn write(&self, data: &[T]) -> usize {
        let reader = self.reader.load(Ordering::Acquire);
        let writer = self.writer.load(Ordering::Acquire);

        let (left, right) = slices_mut(
            self.buffer_ptr(),
            self.buffer_len(),
            writer,
            reader,
        );

        let copied = copy_to_split(data, left, right);

        let writer = (writer + copied) % N;
        self.writer.store(writer, Ordering::Release);

        copied
    }
}

fn copy_to_split<T: Copy>(src: &[T], dst_left: &mut [T], dst_right: &mut [T]) -> usize {
    let nleft = cmp::min(src.len(), dst_left.len());
    dst_left[..nleft].copy_from_slice(&src[..nleft]);

    let src = &src[nleft..];

    let nright = cmp::min(src.len(), dst_right.len());
    dst_right[..nright].copy_from_slice(&src[..nright]);

    nleft + nright
}

fn copy_from_split<T: Copy>(src_left: &[T], src_right: &[T], dst: &mut [T]) -> usize {
    let nleft = cmp::min(dst.len(), src_left.len());
    dst[..nleft].copy_from_slice(&src_left[..nleft]);

    let dst = &mut dst[nleft..];

    let nright = cmp::min(dst.len(), src_right.len());
    dst[..nright].copy_from_slice(&src_right[..nright]);

    nleft + nright
}

unsafe fn slices<'a, T>(
    ring: *const T,
    length: usize,
    start: usize,
    end: usize,
) -> (&'a [T], &'a [T]) {
    if start <= end {
        // simple contiguous case, no wraparound
        let slice = slice::from_raw_parts(ring.add(start), end - start);
        (slice, &[])
    } else {
        // handle wraparound
        // there are two sections: start til len, 0 til end
        let left = slice::from_raw_parts(ring.add(start), length - start);
        let right = slice::from_raw_parts(ring, end);
        (left, right)
    }
}

unsafe fn slices_mut<'a, T>(
    ring: *mut T,
    length: usize,
    start: usize,
    end: usize,
) -> (&'a mut [T], &'a mut [T]) {
    if start <= end {
        // simple contiguous case, no wraparound
        let slice = slice::from_raw_parts_mut(ring.add(start), end - start);
        (slice, &mut [])
    } else {
        // handle wraparound
        // there are two sections: start til len, 0 til end
        let left = slice::from_raw_parts_mut(ring.add(start), length - start);
        let right = slice::from_raw_parts_mut(ring, end);
        (left, right)
    }
}

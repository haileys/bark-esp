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
        Self::new_with_buffer([0u8; N])
    }
}

impl<T: Copy, const N: usize> RingBuffer<T, N> {
    pub const fn new_with_buffer(buffer: [T; N]) -> Self {
        RingBuffer {
            reader: AtomicUsize::new(0),
            writer: AtomicUsize::new(0),
            buffer: UnsafeCell::new(buffer),
        }
    }

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
    #[allow(unused)]
    pub unsafe fn read(&self, data: &mut [T]) -> usize {
        self.read_in_place(|left, right| {
            copy_from_split(left, right, data)
        })
    }

    /// SAFETY: only one task may be reading at any given time
    pub unsafe fn read_in_place(&self, func: impl FnOnce(&[T], &[T]) -> usize) -> usize {
        let reader = self.reader.load(Ordering::Acquire);
        let writer = self.writer.load(Ordering::Acquire);

        let (left, right) = reader_slices(
            self.buffer_ptr(),
            self.buffer_len(),
            reader,
            writer,
        );

        let copied = func(left, right);

        let reader = (reader + copied) % N;
        self.reader.store(reader, Ordering::Release);

        copied
    }

    /// SAFETY: only one task may be writing at any given time
    pub unsafe fn write(&self, data: &[T]) -> usize {
        let reader = self.reader.load(Ordering::Acquire);
        let writer = self.writer.load(Ordering::Acquire);

        // esp_println::println!("writing: reader={reader}, writer={writer}");

        let (left, right) = writer_slices(
            self.buffer_ptr(),
            self.buffer_len(),
            writer,
            reader,
        );

        let available = left.len() + right.len();

        let data = if data.len() >= available {
            // we need to never fill up the entire ringbuffer, since
            // reader == writer indicates that it is empty. so, if we would
            // fill the ring buffer, slice data so that we don't
            &data[0..(available.saturating_sub(1))]
        } else {
            data
        };

        let copied = copy_to_split(data, left, right);

        let writer = (writer + copied) % N;
        self.writer.store(writer, Ordering::Release);

        copied
    }

    // Fills all available space for writing in buffer with copies of T
    pub unsafe fn fill(&self, value: T) {
        let reader = self.reader.load(Ordering::Acquire);
        let writer = self.writer.load(Ordering::Acquire);

        let (left, right) = writer_slices(
            self.buffer_ptr(),
            self.buffer_len(),
            writer,
            reader,
        );

        left.fill(value);
        right.fill(value);

        let copied = (left.len() + right.len()).saturating_sub(1);
        let writer = (writer + copied) % N;
        self.writer.store(writer, Ordering::Release);
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

unsafe fn reader_slices<'a, T>(
    ring: *const T,
    length: usize,
    start: usize,
    end: usize,
) -> (&'a [T], &'a [T]) {
    // we use <= here because start == end means there is no data in the
    // ringbuffer, and so nothing to read:
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

unsafe fn writer_slices<'a, T>(
    ring: *mut T,
    length: usize,
    start: usize,
    end: usize,
) -> (&'a mut [T], &'a mut [T]) {
    if start < end {
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

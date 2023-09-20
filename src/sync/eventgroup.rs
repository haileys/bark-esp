use core::cell::UnsafeCell;
use core::fmt;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::pin::Pin;

use esp_idf_sys as sys;
use bitflags::Flags;

use crate::system::heap::{MallocError, HeapBox};

pub struct EventGroup<T: Flags> {
    cell: UnsafeCell<Inner>,
    _phantom: PhantomData<T>,
}

impl<T: Flags> fmt::Debug for EventGroup<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = unsafe { &*self.cell.get() };
        write!(f, "EventGroup {{ handle: {:x?} }}", inner.handle)
    }
}

struct Inner {
    handle: Option<sys::EventGroupHandle_t>,
    buffer: MaybeUninit<sys::StaticEventGroup_t>,
}

unsafe impl<T: Flags<Bits = u32> + Copy> Sync for EventGroup<T> {}

impl<T: Flags<Bits = u32> + Copy> EventGroup<T> {
    pub const fn declare() -> Self {
        EventGroup {
            cell: UnsafeCell::new(Inner {
                handle: None,
                buffer: MaybeUninit::uninit(),
            }),
            _phantom: PhantomData,
        }
    }

    pub unsafe fn init_with(self: Pin<&Self>, value: T) {
        let cell = &mut *self.cell.get();
        let handle = sys::xEventGroupCreateStatic(cell.buffer.as_mut_ptr());
        sys::xEventGroupSetBits(handle, value.bits());
        cell.handle = Some(handle);
    }

    pub fn boxed(value: T) -> Result<Pin<HeapBox<EventGroup<T>>>, MallocError> {
        let eventgroup = HeapBox::pin(EventGroup::declare())?;
        unsafe { eventgroup.as_ref().init_with(value); }
        Ok(eventgroup)
    }

    fn handle(self: Pin<&Self>) -> sys::EventGroupHandle_t {
        let cell = unsafe { &*self.cell.get() };
        cell.handle.expect("must call EventGroup::init before using!")
    }

    /// Sets the given flags (group |= flags), and returns the flags set at
    /// the time this call returns. See `xEventGroupSetBits`.
    pub fn set(self: Pin<&Self>, flags: T) -> T {
        T::from_bits_retain(unsafe {
            sys::xEventGroupSetBits(self.handle(), flags.bits())
        })
    }

    /// Sets the given flags (group |= flags), and returns the flags set
    /// *before* flags were cleared. See `xEventGroupSetBits`.
    pub fn clear(self: Pin<&Self>, flags: T) -> T {
        T::from_bits_retain(unsafe {
            sys::xEventGroupClearBits(self.handle(), flags.bits())
        })
    }

    #[allow(unused)]
    pub fn wait_all(self: Pin<&Self>, flags: T) -> T {
        T::from_bits_retain(unsafe {
            sys::xEventGroupWaitBits(self.handle(), flags.bits(), 0, 1, sys::freertos_wait_forever)
        })
    }

    pub fn wait_for_any_and_clear(self: Pin<&Self>, flags: T) -> T {
        T::from_bits_retain(unsafe {
            sys::xEventGroupWaitBits(self.handle(), flags.bits(), 1, 0, sys::freertos_wait_forever)
        })
    }

    /// Returns the value of the event group at the time the bits being waited
    /// for became set.
    #[allow(unused)]
    pub fn set_and_then_wait_for(self: Pin<&Self>, set: T, wait_for: T) -> T {
        T::from_bits_retain(unsafe {
            sys::xEventGroupSync(self.handle(), set.bits(), wait_for.bits(), sys::freertos_wait_forever)
        })
    }
}

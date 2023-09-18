use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::mem::MaybeUninit;

use esp_idf_sys as sys;
use bitflags::Flags;

pub struct EventGroup<T: Flags> {
    cell: UnsafeCell<Inner>,
    _phantom: PhantomData<T>,
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

    pub unsafe fn init_with(&'static self, value: T) {
        let cell = &mut *self.cell.get();
        let handle = sys::xEventGroupCreateStatic(cell.buffer.as_mut_ptr());
        sys::xEventGroupSetBits(handle, value.bits());
        cell.handle = Some(handle);
    }

    fn handle(&self) -> sys::EventGroupHandle_t {
        let cell = unsafe { &*self.cell.get() };
        cell.handle.expect("must call EventGroup::init before using!")
    }

    pub fn set(&self, flags: T) {
        unsafe {
            sys::xEventGroupSetBits(self.handle(), flags.bits());
        }
    }

    #[allow(unused)]
    pub fn clear(&self, flags: T) {
        unsafe {
            sys::xEventGroupClearBits(self.handle(), flags.bits());
        }
    }

    #[allow(unused)]
    pub fn wait_all(&self, flags: T) {
        loop {
            let set = unsafe {
                sys::xEventGroupWaitBits(self.handle(), flags.bits(), 0, 1, 1000)
            };

            if T::from_bits_truncate(set).contains(flags) {
                return;
            }
        }
    }

    pub fn wait_for_any_and_clear(&self, flags: T) -> T {
        loop {
            let set = unsafe {
                sys::xEventGroupWaitBits(self.handle(), flags.bits(), 1, 0, 1000)
            };

            if set == 0 {
                // timed out, go again:
                continue;
            }

            return T::from_bits_truncate(set);
        }
    }
}

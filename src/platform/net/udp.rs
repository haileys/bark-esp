use core::pin::Pin;
use core::ptr::{self, NonNull};
use core::net::SocketAddrV4;
use core::ffi::c_void;
use core::mem::ManuallyDrop;

use bitflags::bitflags;
use esp_idf_sys as sys;

use crate::system::heap::{HeapBox, UntypedHeapBox, MallocError};
use crate::sync::EventGroup;

use super::{NetError, LwipError, esp_to_rust_ipv4_addr, rust_to_esp_ipv4_addr};

pub struct Udp {
    udp: UdpPtr,
    eventgroup: Pin<HeapBox<EventGroup<Flags>>>,
    receive_cb: Option<ManuallyDrop<UntypedHeapBox>>,
}

struct HeapCallback<F> {
    eventgroup: *const EventGroup<Flags>,
    func: F,
}

impl<F> HeapCallback<F> {
    pub fn eventgroup(&self) -> Pin<&EventGroup<Flags>> {
        // SAFETY: this event group always lives longer than HeapCallback.
        // it is safely pinned behind a HeapBox too.
        let eventgroup = unsafe { &*self.eventgroup };
        unsafe { Pin::new_unchecked(eventgroup) }
    }
}

bitflags! {
    #[derive(Clone, Copy, Debug)]
    struct Flags: u32 {
        /// Flag set when callback not running.
        /// ie. cleared during callback execution, restored on return
        const CALLBACK_SAFE = 1 << 0;
        /// Flag set when callback is about to be freed.
        /// If the callback should run one last time, this flag indicates that
        /// it should return early rather than proceeding.
        const CALLBACK_STOP = 1 << 1;
    }
}

impl Udp {
    pub fn new() -> Result<Udp, NetError> {
        Ok(Udp {
            udp: UdpPtr::new(sys::lwip_ip_addr_type_IPADDR_TYPE_V4)?,
            eventgroup: EventGroup::boxed(Flags::empty())?,
            receive_cb: None,
        })
    }

    pub fn bind(&mut self, addr: SocketAddrV4) -> Result<(), NetError> {
        let port = addr.port();

        let ip4_addr = rust_to_esp_ipv4_addr(*addr.ip());
        let mut ip_addr = sys::ip_addr::default();
        ip_addr.u_addr.ip4 = ip4_addr;
        ip_addr.type_ = sys::lwip_ip_addr_type_IPADDR_TYPE_V4 as u8;

        Ok(unsafe {
            LwipError::check(sys::udp_bind(self.as_mut_ptr(), &ip_addr, port))?
        })
    }

    fn as_mut_ptr(&mut self) -> *mut sys::udp_pcb {
        self.udp.0.as_ptr()
    }

    fn safely_free_receive_callback(&mut self) {
        if let Some(mut callback) = self.receive_cb.take() {
            // first set stop flag
            self.eventgroup.as_ref().set(Flags::CALLBACK_STOP);

            // then unset receive cb in lwip api
            unsafe {
                sys::udp_recv(
                    self.as_mut_ptr(),
                    None,
                    ptr::null_mut(),
                );
            }

            // if a callback is currently running, wait for it to finish
            self.eventgroup.as_ref().wait_all(Flags::CALLBACK_SAFE);

            // we can free it now:
            unsafe { ManuallyDrop::drop(&mut callback); }

            // reset flags to nothing
            self.eventgroup.as_ref().clear(Flags::all());
        }
    }

    pub fn on_receive<F: FnMut(NonNull<sys::pbuf>, SocketAddrV4)>(&mut self, func: F) -> Result<(), MallocError> {
        let eventgroup = self.eventgroup.as_ref().get_ref();

        let callback = HeapBox::alloc(HeapCallback {
            eventgroup: eventgroup as *const _,
            func,
        })?;

        unsafe extern "C" fn dispatch<F: FnMut(NonNull<sys::pbuf>, SocketAddrV4)>(
            arg: *mut c_void,
            _pcb: *mut sys::udp_pcb,
            pbuf: *mut sys::pbuf,
            addr: *const sys::ip_addr_t,
            port: u16,
        ) {
            let mut callback = NonNull::new_unchecked(arg).cast::<HeapCallback<F>>();

            // synchronisation barrier
            {
                let eventgroup = callback.as_ref().eventgroup();
                let flags = eventgroup.clear(Flags::CALLBACK_SAFE);
                if flags.contains(Flags::CALLBACK_STOP) {
                    eventgroup.set(Flags::CALLBACK_SAFE);
                    return;
                }
            }

            let pbuf = NonNull::<sys::pbuf>::new_unchecked(pbuf);

            let addr = esp_to_rust_ipv4_addr((*addr).u_addr.ip4);
            let addr = SocketAddrV4::new(addr, port);

            // call the callback
            {
                let callback = callback.as_mut();
                (callback.func)(pbuf, addr);
            }

            // set flag indicating we're done
            {
                callback.as_ref().eventgroup().set(Flags::CALLBACK_SAFE);
            }
        }

        // if there's a pre-existing callback, do the whole quiesce process
        self.safely_free_receive_callback();

        // set flags ready for initial callback run:
        self.eventgroup.as_ref().set(Flags::CALLBACK_SAFE);

        // set the callback:
        let callback_ptr = HeapBox::as_borrowed_mut_ptr(&callback);
        let callback_ptr = callback_ptr.cast::<c_void>();
        unsafe {
            sys::udp_recv(
                self.udp.0.as_ptr(),
                Some(dispatch::<F>),
                callback_ptr,
            );
        }

        // save the box on self and return:
        self.receive_cb = Some(ManuallyDrop::new(HeapBox::erase_type(callback)));
        Ok(())
    }
}

impl Drop for Udp {
    fn drop(&mut self) {
        self.safely_free_receive_callback();
    }
}

struct UdpPtr(NonNull<sys::udp_pcb>);

impl UdpPtr {
    pub fn new(ip_type: sys::lwip_ip_addr_type) -> Result<Self, NetError> {
        let ptr = unsafe { sys::udp_new_ip_type(ip_type as u8) };
        NonNull::new(ptr).map(UdpPtr).ok_or(NetError::NewSocket)
    }
}

impl Drop for UdpPtr {
    fn drop(&mut self) {
        // SAFETY: we own it
        unsafe { sys::udp_remove(self.0.as_ptr()); }
    }
}

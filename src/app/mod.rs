use core::ffi::c_void;
use core::net::{SocketAddrV4, Ipv4Addr};
use core::ptr;
use core::sync::atomic::{AtomicU32, Ordering};
use core::time::Duration;

use cstr::cstr;
use esp_idf_sys as sys;

use crate::platform::net;
use crate::system::task;

static PACKETS_RECEIVED: AtomicU32 = AtomicU32::new(0);

const MULTICAST_GROUP: Ipv4Addr = Ipv4Addr::new(224, 100, 100, 100);

pub fn start() {
    task::new(cstr!("bark::app"))
        .spawn(task)
        .expect("spawn app task");
}

pub fn stop() {

}

fn task() {
    log::info!("Starting application");
    crate::system::task::log_tasks();

    unsafe {
        let udp = sys::udp_new_ip_type(sys::lwip_ip_addr_type_IPADDR_TYPE_V4 as u8);
        if udp == ptr::null_mut() {
            log::error!("failed to allocate udp_pcb");
            return;
        }

        sys::udp_recv(udp, Some(receive_packet), ptr::null_mut());

        if sys::udp_bind(udp, &sys::ip_addr_any, 1530) != 0 {
            log::error!("failed to udp_bind");
            return;
        }

        // if let Err(e) = net::join_multicast_group(MULTICAST_GROUP) {
        //     log::error!("failed to join multicast group: {MULTICAST_GROUP}: {e:?}");
        //     return;
        // }
    }

    loop {
        task::delay(Duration::from_millis(500));
        // log::info!("packets received: {}", PACKETS_RECEIVED.load(Ordering::Relaxed));
    }
}

unsafe extern "C" fn receive_packet(
    _arg: *mut c_void,
    _pcb: *mut sys::udp_pcb,
    buf: *mut sys::pbuf,
    addr: *const sys::ip_addr_t,
    port: u16,
) {
    // addr might point into buff, so take a copy early:
    let addr = *addr;
    let addr = SocketAddrV4::new(Ipv4Addr::from(addr.u_addr.ip4.addr), port);

    {
        let buf = &*buf;

        PACKETS_RECEIVED.fetch_add(1, Ordering::Relaxed);
        log::info!("received udp packet from {addr}: len={len}, tot_len={tot_len}",
            len = buf.len,
            tot_len = buf.tot_len,
        );
    }

    unsafe { sys::pbuf_free(buf); }
}

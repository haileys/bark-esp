use core::ptr;
use core::net::Ipv4Addr;

use derive_more::From;
use esp_idf_sys as sys;

use crate::system::heap::MallocError;

pub mod udp;

pub fn join_multicast_group(group: Ipv4Addr) -> Result<(), NetError> {
    log::info!("Joining multicast group {group}");

    let netif = netif()?;
    let addr = rust_to_esp_ipv4_addr(group);

    LwipError::check(unsafe {
        sys::igmp_joingroup_netif(netif, &addr)
    })?;

    Ok(())
}

#[allow(unused)]
pub fn leave_multicast_group(group: Ipv4Addr) -> Result<(), NetError> {
    log::info!("Joining multicast group {group}");

    let netif = netif()?;
    let addr = rust_to_esp_ipv4_addr(group);

    LwipError::check(unsafe {
        sys::igmp_leavegroup_netif(netif, &addr)
    })?;

    Ok(())
}

pub fn rust_to_esp_ipv4_addr(addr: Ipv4Addr) -> sys::ip4_addr {
    let octets = addr.octets();

    let addr = ((octets[3] as u32) << 24)
             | ((octets[2] as u32) << 16)
             | ((octets[1] as u32) << 8)
             | ((octets[0] as u32) << 0)
             ;

    sys::ip4_addr { addr }
}

pub fn esp_to_rust_ipv4_addr(addr: sys::ip4_addr) -> Ipv4Addr {
    let addr = addr.addr;

    let octet0 = ((addr >> 0) & 0xff) as u8;
    let octet1 = ((addr >> 8) & 0xff) as u8;
    let octet2 = ((addr >> 16) & 0xff) as u8;
    let octet3 = ((addr >> 24) & 0xff) as u8;

    Ipv4Addr::new(octet0, octet1, octet2, octet3)
}

fn netif() -> Result<*mut sys::netif, NetError> {
    let esp_netif = unsafe { sys::esp_netif_get_default_netif() };
    if esp_netif == ptr::null_mut() {
        return Err(NetError::NoNetif);
    }

    let netif = unsafe { sys::esp_netif_get_netif_impl(esp_netif) };
    if netif == ptr::null_mut() {
        return Err(NetError::NoNetif);
    }

    Ok(netif as *mut sys::netif)
}

#[derive(Debug, From)]
pub enum NetError {
    NoNetif,
    NewSocket,
    Alloc(MallocError),
    Lwip(LwipError),
}

#[derive(Debug)]
pub struct LwipError(i8);

impl LwipError {
    #[allow(unused)]
    pub fn code(&self) -> i8 {
        self.0
    }

    pub fn check(rc: i8) -> Result<(), Self> {
        match rc {
            0 => Ok(()),
            _ => Err(LwipError(rc))
        }
    }
}

use core::net::Ipv4Addr;

use bark_protocol::packet::PacketKind;
use bark_protocol::types::{TimePhase, TimestampMicros};
use cstr::cstr;
use derive_more::From;
use esp_idf_sys as sys;
use static_assertions::{const_assert, const_assert_eq};

use bark_protocol::buffer::pbuf as bark_pbuf;

use crate::system::task;

mod protocol;
mod stream;

use protocol::Protocol;

use self::protocol::{BindError, SocketError};

// statically assert that the bark pbuf type is compatible with esp-idf's
const_assert_eq!(
    bark_pbuf::ffi::PBUF_RAM,
    sys::pbuf_type_PBUF_RAM);
const_assert!(
    bark_pbuf::ffi::PBUF_TRANSPORT
    >= sys::pbuf_layer_PBUF_TRANSPORT);

const MULTICAST_GROUP: Ipv4Addr = Ipv4Addr::new(224, 100, 100, 100);
const MULTICAST_PORT: u16 = 1530;

pub fn start() {
    task::new(cstr!("bark::app"))
        .spawn(task)
        .expect("spawn app task");
}

pub fn stop() {

}

#[derive(Debug, From)]
pub enum AppError {
    Bind(BindError),
    Socket(SocketError),
}

async fn task() -> Result<(), AppError> {
    log::info!("Starting application");
    log::info!("PBUF_TRANSPORT = {}", sys::pbuf_layer_PBUF_TRANSPORT);

    crate::system::task::log_tasks();

    let mut protocol = Protocol::bind(MULTICAST_GROUP, MULTICAST_PORT)?;

    loop {
        let (packet, addr) = match protocol.receive().await {
            Ok(result) => result,
            Err(e) => {
                log::warn!("error receiving protocol packet: {e:?}");
                continue;
            }
        };

        match packet {
            PacketKind::Time(mut time) => {
                match time.data().phase() {
                    Some(TimePhase::Broadcast) => {
                        let data = time.data_mut();
                        data.receive_2 = timestamp();
                        protocol.send(time.as_packet(), addr)?;
                    }
                    Some(TimePhase::StreamReply) => {
                        log::info!("received stream reply time packet!");
                    }
                    _ => { /* invalid */ }
                }
            }
            _ => {}
        }
    }
}

fn timestamp() -> TimestampMicros {
    let micros: i64 = unsafe { sys::esp_timer_get_time() };
    let micros: u64 = micros.try_into().expect("negative timestamp from esp_timer_get_time");
    TimestampMicros(micros)
}

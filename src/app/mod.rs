use core::net::Ipv4Addr;

use bark_protocol::packet::PacketKind;
use bark_protocol::types::{TimePhase, TimestampMicros, SessionId};
use derive_more::From;
use esp_idf_sys as sys;
use static_assertions::{const_assert, const_assert_eq};

use bark_protocol::buffer::pbuf as bark_pbuf;

use crate::system::task;

mod consts;
mod protocol;
mod stream;
mod timing;
mod queue;

use protocol::{Protocol, BindError, SocketError};
use stream::Stream;

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
    task::new("bark::app")
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

    let mut protocol = Protocol::bind(MULTICAST_GROUP, MULTICAST_PORT)?;
    let mut receiver = Receiver::new();

    loop {
        let (packet, addr) = match protocol.receive().await {
            Ok(result) => result,
            Err(e) => {
                log::warn!("error receiving protocol packet: {e:?}");
                continue;
            }
        };

        match packet {
            PacketKind::Audio(audio) => {
                let header = audio.header();
                let stream = receiver.prepare_stream(header.sid, header.seq);
                if let Some(stream) = stream {
                    stream.receive_audio(audio).await;
                }
            }
            PacketKind::Time(mut time) => {
                match time.data().phase() {
                    Some(TimePhase::Broadcast) => {
                        let data = time.data_mut();
                        data.receive_2 = timestamp();
                        protocol.send(time.as_packet(), addr)?;
                    }
                    Some(TimePhase::StreamReply) => {
                        let data = time.data();
                        if let Some(stream) = receiver.get_stream(data.sid) {
                            stream.receive_time(time);
                        }
                    }
                    _ => { /* invalid packet */ }
                }
            }
            _ => {
                log::warn!("received unhandled packet kind: {packet:?}");
            }
        }
    }
}

pub struct Receiver {
    stream: Option<Stream>,
}

impl Receiver {
    pub fn new() -> Self {
        Receiver { stream: None }
    }

    fn get_stream(&mut self, sid: SessionId) -> Option<&mut Stream> {
        self.stream.as_mut().filter(|stream| stream.sid() == sid)
    }

    /// Resets current stream if necessary.
    fn prepare_stream(&mut self, sid: SessionId, seq: u64) -> Option<&mut Stream> {
        let new_stream = match &self.stream {
            Some(stream) => stream.sid() < sid,
            None => true,
        };

        if new_stream {
            match Stream::new(sid, seq) {
                Ok(stream) => { self.stream = Some(stream); }
                Err(e) => {
                    log::warn!("failed to allocate new stream: {e:?}");
                    self.stream = None;
                }
            }
        }

        self.stream.as_mut()
    }
}

fn timestamp() -> TimestampMicros {
    let micros: i64 = unsafe { sys::esp_timer_get_time() };
    let micros: u64 = micros.try_into().expect("negative timestamp from esp_timer_get_time");
    TimestampMicros(micros)
}

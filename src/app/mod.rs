use core::net::Ipv4Addr;

use bark_protocol::packet::PacketKind;
use bark_protocol::types::{TimePhase, TimestampMicros, SessionId};
use derive_more::From;
use esp_idf_sys as sys;
use static_assertions::{const_assert, const_assert_eq};

use bark_protocol::buffer::pbuf as bark_pbuf;

use crate::platform::dac::{Dac, DacError, NewDacError};
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

    task::new("bark::app::dac")
        .spawn(dac_task)
        .expect("spawn app dac task");
}

pub fn stop() {

}

#[derive(Debug, From)]
pub enum AppError {
    Bind(BindError),
    Socket(SocketError),
    OpenDac(NewDacError),
}

async fn task() -> Result<(), AppError> {
    log::info!("Starting application");
    log::info!("PBUF_TRANSPORT = {}", sys::pbuf_layer_PBUF_TRANSPORT);

    crate::system::task::log_tasks();

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
                    stream.receive_audio(audio);
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

#[derive(Debug)]
pub enum DacTaskError {
    Open(NewDacError),
    Enable(DacError),
    StartAsync(DacError),
    Write(DacError),
}

async fn dac_task() -> Result<(), DacTaskError> {
    const SAMPLE_RATE: u32 = 48000;
    const HZ: u32 = 50;

    let mut dac = Dac::new()
        .map_err(DacTaskError::Open)?;

    dac.enable()
        .map_err(DacTaskError::Enable)?;

    dac.start_async_writing()
        .map_err(DacTaskError::StartAsync)?;

    let mut t: u32 = 0;
    let mut buff = [0u8; 1024];

    loop {
        // fill buffer with sawtooth wave:
        for [left, right] in buff.array_chunks_mut() {
            // increment frame time
            t += 1;
            // calculate value for this frame
            let val = (t * HZ * 256) / SAMPLE_RATE;
            // convert to u8
            let val = val as u8;
            // write to buffer
            *left = val;
            *right = val;
        }

        // write buffer to dac
        dac.write(&buff).await
            .map_err(DacTaskError::Write)?;
    }
}

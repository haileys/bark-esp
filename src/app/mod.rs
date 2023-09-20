use core::mem::size_of;
use core::net::{SocketAddrV4, Ipv4Addr};
use core::sync::atomic::{AtomicU32, Ordering};

use bark_protocol::packet::{Packet, PacketKind};
use cstr::cstr;
use memoffset::offset_of;
use esp_idf_sys as sys;
use static_assertions::{const_assert, const_assert_eq};

use bark_protocol::buffer::{PacketBuffer, AllocError};
use bark_protocol::buffer::pbuf as bark_pbuf;

use crate::platform::net;
use crate::sync::streambuffer;
use crate::system::heap::MallocError;
use crate::system::task;

// statically assert that the bark pbuf type is compatible with esp-idf's
const_assert_eq!(
    bark_pbuf::ffi::PBUF_RAM,
    sys::pbuf_type_PBUF_RAM);
const_assert!(
    bark_pbuf::ffi::PBUF_TRANSPORT
    >= sys::pbuf_layer_PBUF_TRANSPORT as usize);
const_assert_eq!(
    offset_of!(bark_pbuf::ffi::pbuf, payload),
    offset_of!(sys::pbuf, payload));
const_assert_eq!(
    offset_of!(bark_pbuf::ffi::pbuf, len),
    offset_of!(sys::pbuf, len));

static AUDIO_PACKETS_RECEIVED: AtomicU32 = AtomicU32::new(0);

const MULTICAST_GROUP: Ipv4Addr = Ipv4Addr::new(224, 100, 100, 100);
const MULTICAST_PORT: u16 = 1530;

pub fn start() {
    task::new(cstr!("bark::app"))
        .spawn(task)
        .expect("spawn app task");
}

pub fn stop() {

}

#[derive(Debug)]
pub enum AppError {
    NewSocket(net::NetError),
    AllocateStreamBuffer(MallocError),
    SetOnReceiveCallback(MallocError),
    BindSocket(net::NetError),
    JoinMulticastGroup(net::NetError),
}

async fn task() -> Result<(), AppError> {
    log::info!("Starting application");
    log::info!("PBUF_TRANSPORT = {}", sys::pbuf_layer_PBUF_TRANSPORT);

    crate::system::task::log_tasks();

    let mut socket = net::udp::Udp::new()
        .map_err(AppError::NewSocket)?;

    let (mut packet_tx, mut packet_rx) = streambuffer::channel(16)
        .map_err(AppError::AllocateStreamBuffer)?;

    socket.on_receive(move |pbuf, addr| {
        let pbuf = pbuf.cast::<bark_pbuf::ffi::pbuf>();
        let pbuf = unsafe { bark_pbuf::BufferImpl::from_raw(pbuf) };
        let buffer = PacketBuffer::from_underlying(pbuf);

        match align_packet_buffer(buffer) {
            Ok(buffer) => {
                match packet_tx.try_send((buffer, addr)) {
                    Ok(()) => {}
                    Err(_) => {
                        // failed to write to stream buffer!!
                        // the app task must be failing to keep up, nothing we
                        // can do here but drop the packet
                    }
                }
            },
            Err(_) => {
                // failed to allocate!! this is probably the most likely
                // place we'll fail to allocate in bark-esp: apart from the
                // wifi packet buffer, which we do try to reuse if aligned,
                // it is the only data plane allocation we do.
                //
                // quietly drop the error for now, but TODO we need to report
                // it somehow, we should avoid logging in this callback.
            }
        }
    }).map_err(AppError::SetOnReceiveCallback)?;

    socket.bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, MULTICAST_PORT))
        .map_err(AppError::BindSocket)?;

    net::join_multicast_group(MULTICAST_GROUP)
        .map_err(AppError::JoinMulticastGroup)?;

    loop {
        let (buffer, addr) = packet_rx.receive().await;
        receive_packet_buffer(buffer, addr);
        // task::delay(Duration::from_millis(500));
        // log::info!("audio packets received: {}", AUDIO_PACKETS_RECEIVED.load(Ordering::Relaxed));
    }
}

fn receive_packet_buffer(buffer: PacketBuffer, addr: SocketAddrV4) {
    let packet = Packet::from_buffer(buffer)
        .and_then(|packet| packet.parse());

    match packet {
        None => { return; }
        Some(PacketKind::Audio(_)) => {
            AUDIO_PACKETS_RECEIVED.fetch_add(1, Ordering::SeqCst);
        }
        Some(_) => {
            log::info!("received packet from {addr}: {packet:?}")
        }
    }
}

fn align_packet_buffer(buffer: PacketBuffer) -> Result<PacketBuffer, AllocError> {
    let align_offset = buffer.as_bytes().as_ptr() as usize % size_of::<u32>();

    if align_offset == 0 {
        // already aligned, nothing to do:
        return Ok(buffer);
    }

    // packet is not aligned :( we have to reallocate + move it
    let mut aligned_buffer = PacketBuffer::allocate(buffer.len())?;

    // copy from the unaligned buffer into the aligned buffer:
    aligned_buffer.as_bytes_mut().copy_from_slice(buffer.as_bytes());

    // drop the unaligned buffer:
    drop(buffer);

    Ok(aligned_buffer)
}

use core::alloc::Layout;
use core::net::{Ipv4Addr, SocketAddrV4};

use bark_protocol::buffer::pbuf as bark_pbuf;
use bark_protocol::buffer::{AllocError, PacketBuffer};
use bark_protocol::packet::{Packet, PacketKind};
use derive_more::From;
use esp_pbuf::PbufUninit;

use crate::platform::net;
use crate::platform::net::NetError;
use crate::platform::net::udp::Udp;
use crate::sync::queue::{self, QueueReceiver, AllocQueueError};
use crate::system::heap::MallocError;

pub struct Protocol {
    socket: Udp,
    packet_rx: QueueReceiver<Result<(PacketBuffer, SocketAddrV4), AllocError>>,
}

#[derive(Debug)]
pub enum BindError {
    NewSocket(net::NetError),
    AllocatePacketQueue(AllocQueueError),
    SetOnReceiveCallback(MallocError),
    BindSocket(net::NetError),
    JoinMulticastGroup(net::NetError),
}

#[derive(Debug, From)]
pub enum SocketError {
    AllocatePacketBuffer(AllocError),
    Net(NetError),
}

impl Protocol {
    pub fn bind(group: Ipv4Addr, port: u16) -> Result<Self, BindError> {
        let mut socket = net::udp::Udp::new()
            .map_err(BindError::NewSocket)?;

        let (mut packet_tx, packet_rx) = queue::channel(16)
            .map_err(BindError::AllocatePacketQueue)?;

        socket.on_receive(move |pbuf, addr| {
            let buffer = PacketBuffer::from_raw(pbuf);

            let result = align_packet_buffer(buffer)
                .map(|buffer| (buffer, addr));

            match packet_tx.try_send(result) {
                Ok(()) => {}
                Err(_) => {
                    // failed to write to stream buffer!!
                    // the app task must be failing to keep up, nothing we
                    // can do here but drop the packet
                }
            }
        }).map_err(BindError::SetOnReceiveCallback)?;

        socket.bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port))
            .map_err(BindError::BindSocket)?;

        net::join_multicast_group(group)
            .map_err(BindError::JoinMulticastGroup)?;

        Ok(Protocol {
            socket,
            packet_rx,
        })
    }

    pub async fn receive(&mut self) -> Result<(PacketKind, SocketAddrV4), SocketError> {
        loop {
            let (buffer, addr) = self.packet_rx.receive().await?;
            let Some(packet) = Packet::from_buffer(buffer) else { continue };
            let Some(packet) = packet.parse() else { continue };
            return Ok((packet, addr));
        }
    }

    pub fn send(&mut self, packet: &Packet, addr: SocketAddrV4) -> Result<(), SocketError> {
        self.socket.send_to(packet.as_buffer(), addr)?;
        Ok(())
    }
}

// This is unfortunate but necessary for now. We need to reallocate+copy
// packet buffer contents to make sure the start of a bark protocol packet
// is aligned. TODO - see if we can coax the network stack into giving us
// properly aligned packets.
fn align_packet_buffer(buffer: PacketBuffer) -> Result<PacketBuffer, AllocError> {
    const ALIGN: usize = core::mem::size_of::<u64>();

    let align_offset = buffer.as_bytes().as_ptr() as usize % ALIGN;

    if align_offset == 0 {
        // already aligned, nothing to do:
        return Ok(buffer);
    }

    // packet is not aligned :( we have to reallocate + move it
    let pbuf = PbufUninit::allocate_layout(
        bark_pbuf::ffi::PBUF_TRANSPORT,
        bark_pbuf::ffi::PBUF_RAM,
        Layout::from_size_align(buffer.len(), ALIGN).unwrap(),
    ).map_err(AllocError)?;

    let pbuf = pbuf.copied_from_slice(buffer.as_bytes());

    Ok(PacketBuffer::from_raw(pbuf))
}

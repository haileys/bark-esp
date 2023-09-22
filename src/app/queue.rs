use heapless::Deque;

use bark_protocol::packet::Audio;

use crate::system::heap::{SharedBox, MallocError};
use crate::sync::mutex::Mutex;

use super::consts::MAX_QUEUED_PACKETS;

#[derive(Clone)]
pub struct PacketQueue {
    shared: SharedBox<Mutex<Shared>>,
}

impl PacketQueue {
    pub fn disconnected(&self) -> bool {
        SharedBox::unique(&self.shared)
    }

    pub fn new(start_seq: u64) -> Result<Self, MallocError> {
        let shared = Shared::new(start_seq);
        let shared = SharedBox::alloc(Mutex::new(shared))?;
        Ok(PacketQueue { shared })
    }

    pub fn receive_packet(&self, packet: Audio) {
        let mut shared = self.shared.lock();
        shared.insert_packet(packet);
    }

    pub fn pop_front(&self) -> Option<Audio> {
        let mut shared = self.shared.lock();
        shared.pop_front()
    }
}

struct Shared {
    queue: Deque<Option<Audio>, MAX_QUEUED_PACKETS>,
    /// The seq of the first packet in the queue, the rest are implied
    head_seq: u64,
}

enum NoSlot {
    InPast,
    TooFarInFuture,
}

impl Shared {
    pub fn new(start_seq: u64) -> Shared {
        Shared {
            queue: Deque::default(),
            head_seq: start_seq
        }
    }

    pub fn insert_packet(&mut self, packet: Audio) {
        let packet_seq = packet.header().seq;
        let head_seq = self.head_seq;
        let tail_seq = self.head_seq + self.queue.capacity() as u64;

        match self.queue_slot_mut(packet_seq) {
            Ok(slot@&mut None) => {
                *slot = Some(packet);
            }
            Ok(Some(_)) => {
                log::warn!("received duplicate packet, retaining first received: packet_seq={packet_seq}");
            }
            Err(NoSlot::InPast) => {
                log::warn!("received packet in past, dropping: head_seq={head_seq}, packet_seq={packet_seq}");
            }
            Err(NoSlot::TooFarInFuture) => {
                log::warn!("received packet too far in future, dropping: tail_seq={tail_seq}, packet_seq={packet_seq}");
            }
        }
    }

    pub fn pop_front(&mut self) -> Option<Audio> {
        self.queue.pop_front().flatten()
    }

    fn head_seq(&self) -> u64 {
        self.head_seq
    }

    fn tail_seq(&self) -> u64 {
        self.head_seq + self.queue.capacity() as u64
    }

    fn queue_slot_mut(&mut self, seq: u64) -> Result<&mut Option<Audio>, NoSlot> {
        let idx = seq.checked_sub(self.head_seq).ok_or(NoSlot::InPast)? as usize;

        if idx >= self.queue.capacity() {
            return Err(NoSlot::TooFarInFuture);
        }

        // expand deq if needed so we can take mut ref
        while self.queue.len() <= idx {
            let Ok(()) = self.queue.push_back(None) else {
                unreachable!("bounds check above implies this push always succeeds")
            };
        }

        let slices = self.queue.as_mut_slices();

        if idx < slices.0.len() {
            Ok(&mut slices.0[idx])
        } else {
            Ok(&mut slices.1[idx - slices.0.len()])
        }
    }
}


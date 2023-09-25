use bark_protocol::{SAMPLES_PER_PACKET, FRAMES_PER_PACKET};
use derive_more::From;

use bark_protocol::packet::{Time, Audio};
use bark_protocol::types::SessionId;

use crate::platform::dac::{Dac, DacError, NewDacError, Frame};
use crate::stats::STATS;
use crate::system::heap::MallocError;
use crate::system::task::{self, SpawnError};

use super::consts::DELAY_START_PACKETS;
use super::timing::Timing;
use super::queue::PacketQueue;

#[allow(unused)]
pub struct Stream {
    sid: SessionId,
    timing: Timing,
    queue: PacketQueue,
    start: BufferStart,
}

enum BufferStart {
    ReceivingPackets(usize),
    Started,
}

#[derive(Debug, From)]
pub enum NewStreamError {
    AllocatePacketQueue(MallocError),
    SpawnAudioTask(SpawnError),
}

impl Stream {
    pub fn new(sid: SessionId, seq: u64) -> Result<Self, NewStreamError> {
        let queue = PacketQueue::new(seq)?;

        Ok(Stream {
            sid,
            timing: Timing::default(),
            queue,
            start: BufferStart::ReceivingPackets(0),
        })
    }

    pub fn sid(&self) -> SessionId {
        self.sid
    }

    pub fn receive_time(&mut self, packet: Time) {
        self.timing.receive_packet(packet);
    }

    pub async fn receive_audio(&mut self, packet: Audio) {
        self.queue.receive_packet(packet).await;

        if let BufferStart::ReceivingPackets(count) = &mut self.start {
            *count += 1;
            if *count > DELAY_START_PACKETS {
                self.start_task();
            }
        }
    }

    fn start_task(&mut self) {
        task::new("bark::stream")
            .priority(16)
            .use_alternate_core()
            .spawn({
                let queue = self.queue.clone();
                || async move { run_stream(queue).await }
            })
            .unwrap();

        self.start = BufferStart::Started;
    }
}

#[derive(Debug, From)]
enum AudioTaskError {
    OpenDac(NewDacError),
    Dac(DacError)
}

static SILENCE: [f32; SAMPLES_PER_PACKET] = [0.0; SAMPLES_PER_PACKET];

async fn run_stream(queue: PacketQueue) -> Result<(), AudioTaskError> {
    let mut dac = Dac::new()?;
    dac.enable()?;
    dac.start_async_writing()?;

    let mut buff = [Frame::default(); FRAMES_PER_PACKET];

    loop {
        if queue.disconnected() {
            log::warn!("PacketQueue has disconnected! stream task exiting");
            break;
        }

        let packet = queue.pop_front().await;

        match packet {
            Some(_) => { STATS.stream_hit.increment(); }
            None => { STATS.stream_miss.increment(); }
        }

        let audio = packet.as_ref()
            .map(|packet| packet.buffer())
            .unwrap_or(&SILENCE);

        let audio_f32bits = unsafe { core::mem::transmute::<&[f32], &[u32]>(audio) };

        for i in 0..FRAMES_PER_PACKET {
            let l = convint8(audio_f32bits[i * 2 + 0]);
            let r = convint8(audio_f32bits[i * 2 + 1]);
            buff[i] = Frame(l, r);
        }

        dac.write(&buff).await?;
        // unsafe { esp_idf_sys::vTaskDelay(1); }
    }

    Ok(())
}

fn convint8(bits: u32) -> i8 {
    // extract fields from f32
    let frac = bits & 0x7fffff;
    let exp = (bits >> 23) & 0xff;
    let sign = if (bits >> 31) != 0 { -1 } else { 1 };

    // special case to make our lives easier (zero has a very different
    // canonical form than other floats in [-1.0, 1.0])
    if exp == 0 && frac == 0 {
        return 0;
    }

    // OR on implicit top 1 bit
    let frac = frac | 0x800000;

    if exp < 0x78 {
        // very small
        return 0;
    }

    if exp > 0x86 {
        // clip
        return 127 * sign;
    }

    let normal =
        if exp <= 0x7f {
            frac >> (0x7f - exp)
        } else {
            frac << (exp - 0x7f)
        };

    sign * ((normal - (1<<8)) >> 16) as i8
}

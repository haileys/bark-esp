use bark_protocol::SAMPLES_PER_PACKET;
use derive_more::From;

use bark_protocol::packet::{Time, Audio};
use bark_protocol::types::SessionId;

use crate::platform::dac::{Dac, DacError, NewDacError};
use crate::system::heap::MallocError;
use crate::system::task::{self, SpawnError};

use super::timing::Timing;
use super::queue::PacketQueue;

#[allow(unused)]
pub struct Stream {
    sid: SessionId,
    timing: Timing,
    queue: PacketQueue,
}

#[derive(Debug, From)]
pub enum NewStreamError {
    AllocatePacketQueue(MallocError),
    SpawnAudioTask(SpawnError),
}

impl Stream {
    pub fn new(sid: SessionId, seq: u64) -> Result<Self, NewStreamError> {
        let queue = PacketQueue::new(seq)?;

        task::new("bark::stream::audio")
            .priority(16)
            .use_alternate_core()
            .spawn({
                let queue = queue.clone();
                || async move { run_stream(queue).await }
            })?;

        Ok(Stream {
            sid,
            timing: Timing::default(),
            queue,
        })
    }

    pub fn sid(&self) -> SessionId {
        self.sid
    }

    pub fn receive_time(&mut self, packet: Time) {
        self.timing.receive_packet(packet);
    }

    pub fn receive_audio(&mut self, packet: Audio) {
        self.queue.receive_packet(packet);
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

    let mut buff = [0u8; SAMPLES_PER_PACKET];

    loop {
        if queue.disconnected() {
            break;
        }

        let packet = queue.pop_front();

        let audio = packet.as_ref()
            .map(|packet| packet.buffer())
            .unwrap_or(&SILENCE);

        for i in 0..SAMPLES_PER_PACKET {
            // load float32 sample in range [-1.0, 1.0]
            let mut sample = audio[i];
            // translate to range [0.0, 2.0]
            sample += 1.0;
            // scale to range [0.0, 255.0]
            sample *= 255.0 / 2.0;
            // convert to u8 and store in output buffer
            buff[i] = sample as u8;
        }

        dac.write(&buff).await?;
    }

    Ok(())
}

pub const MAX_QUEUED_PACKETS: usize = 50;

const DELAY_START_MS: usize = 50;
const DELAY_START_SAMPLES: usize = (DELAY_START_MS * bark_protocol::SAMPLE_RATE.0 as usize) / 1000;
pub const DELAY_START_PACKETS: usize = DELAY_START_SAMPLES / bark_protocol::FRAMES_PER_PACKET;

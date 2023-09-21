use bark_protocol::packet::Time;
use bark_protocol::types::SessionId;

use super::timing::Timing;

#[allow(unused)]
pub struct Stream {
    sid: SessionId,
    timing: Timing,
}

impl Stream {
    #[allow(unused)]
    pub fn receive_time(&mut self, packet: Time) {
        todo!();
    }
}

use core::sync::atomic::{AtomicU32, Ordering};
use esp_idf_sys as sys;
use esp_println::println;
use crate::system::task;

pub static STATS: Stats = Stats::new();

pub struct Stats {
    pub wifi_packets_received: Counter,
    pub packets_dropped_in_protocol_queue: Counter,
    pub audio_packets_received_on_time: Counter,
    pub audio_packets_received_late: Counter,
    pub audio_packets_received_early: Counter,
    pub stream_hit: Counter,
    pub stream_miss: Counter,
    pub dac_frames_sent: Counter,
    pub dac_underruns: Counter,
}

impl Stats {
    pub const fn new() -> Self {
        Stats {
            wifi_packets_received: Counter::new(),
            packets_dropped_in_protocol_queue: Counter::new(),
            audio_packets_received_on_time: Counter::new(),
            audio_packets_received_late: Counter::new(),
            audio_packets_received_early: Counter::new(),
            stream_hit: Counter::new(),
            stream_miss: Counter::new(),
            dac_frames_sent: Counter::new(),
            dac_underruns: Counter::new(),
        }
    }
}

#[derive(Default)]
pub struct Counter {
    value: AtomicU32,
}

impl Counter {
    pub const fn new() -> Self {
        Counter { value: AtomicU32::new(0) }
    }

    pub fn increment(&self) {
        self.add(1);
    }

    pub fn add(&self, n: u32) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }

    pub fn take(&self) -> u32 {
        self.value.swap(0, Ordering::Relaxed)
    }
}

pub fn start() {
    task::new("bark::stats")
        .spawn(task)
        .unwrap();
}

async fn task() {
    loop {
        unsafe { sys::vTaskDelay(1000); }

        println!();

        println!(
            "Network:[recv:{}/s queue_drop:{}/s]",
            STATS.wifi_packets_received.take(),
            STATS.packets_dropped_in_protocol_queue.take(),
        );

        println!(
            "Queue:[on_time:{}/s late:{}/s early:{}/s]",
            STATS.audio_packets_received_on_time.take(),
            STATS.audio_packets_received_late.take(),
            STATS.audio_packets_received_early.take()
        );

        println!(
            "Stream:[hit:{}/s miss:{}/s]",
            STATS.stream_hit.take(),
            STATS.stream_miss.take(),
        );

        println!(
            "DAC:[frames_sent:{}/s underruns:{}/s]",
            STATS.dac_frames_sent.take(),
            STATS.dac_underruns.take(),
        )
    }
}

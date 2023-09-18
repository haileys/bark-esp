use core::sync::atomic::Ordering;

use bitflags::bitflags;
use cstr::cstr;

use crate::{sync::EventGroup, system::task};

pub mod eventloop;
pub mod nvs;
pub mod wifi;

pub unsafe fn init() {
    EVENT.init_with(PlatformEvent::empty());

    eventloop::init();
    nvs::init();
    wifi::init();

    task::new(cstr!("bark::platform"))
        .spawn(platform_task)
        .expect("spawn platform task");
}

bitflags! {
    #[derive(Clone, Copy)]
    pub struct PlatformEvent: u32 {
        const WIFI    = 1 << 0;
    }
}

static EVENT: EventGroup<PlatformEvent> = EventGroup::declare();

pub fn raise_event(event: PlatformEvent) {
    EVENT.set(event);
}

fn platform_task() {
    loop {
        let events = EVENT.wait_for_any_and_clear(PlatformEvent::all());

        if events.contains(PlatformEvent::WIFI) {
            on_wifi_event();
        }
    }
}

fn on_wifi_event() {
    let state = wifi::STATE.load(Ordering::SeqCst);
    log::info!("Wifi event! current wifi state: {state:?}")
}

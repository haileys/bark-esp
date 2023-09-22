use core::pin::Pin;
use core::sync::atomic::Ordering;

use bitflags::bitflags;

use crate::platform::wifi::WifiState;
use crate::sync::EventGroup;
use crate::system::task;

pub mod dac;
pub mod eventloop;
pub mod net;
pub mod nvs;
pub mod wifi;

bitflags! {
    #[derive(Clone, Copy)]
    pub struct PlatformEvent: u32 {
        const WIFI    = 1 << 0;
    }
}

static EVENT: EventGroup<PlatformEvent> = EventGroup::declare();

pub unsafe fn init() {
    Pin::static_ref(&EVENT).init_with(PlatformEvent::empty());

    eventloop::init();
    nvs::init();
    wifi::init();

    task::new("bark::platform")
        .spawn(platform_task)
        .expect("spawn platform task");
}

pub fn raise_event(event: PlatformEvent) {
    Pin::static_ref(&EVENT).set(event);
}

async fn platform_task() {
    loop {
        let events = Pin::static_ref(&EVENT)
            .wait_for_any_and_clear(PlatformEvent::all());

        if events.contains(PlatformEvent::WIFI) {
            on_wifi_event();
        }
    }
}

fn on_wifi_event() {
    let state = wifi::STATE.load(Ordering::SeqCst);
    log::info!("Wifi event! current wifi state: {state:?}");

    match state {
        WifiState::Online => { crate::app::start(); }
        WifiState::Disconnected => { crate::app::stop(); }
        _ => {}
    }
}

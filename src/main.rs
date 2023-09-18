#![no_std]
#![no_main]
#![feature(sync_unsafe_cell)]
#![feature(type_alias_impl_trait)]

mod platform;
mod sync;
mod system;

use platform::network::Netif;

#[no_mangle]
pub unsafe extern "C" fn app_main() {
    system::init();
    log::info!("System initialized");

    system::rt::spawner().must_spawn(init());
}

#[embassy_executor::task]
async fn init() {
    unsafe { platform::init() }.await;
    log::info!("Platform initialized");

    let netif = Netif::create_default_station();
    core::mem::forget(netif);

    system::task::log_tasks();
}

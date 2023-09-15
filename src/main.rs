#![no_std]
#![no_main]

mod system;

use system::network::Netif;

#[no_mangle]
pub unsafe extern "C" fn app_main() {
    system::init();
    log::info!("System initialized");

    let netif = Netif::create_default_station();

    system::task::log_tasks();
}

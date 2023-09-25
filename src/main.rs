#![no_std]
#![no_main]
#![feature(array_chunks)]
#![feature(core_intrinsics)]
#![feature(ip_in_core)]
#![feature(sync_unsafe_cell)]
#![feature(type_alias_impl_trait)]
#![feature(waker_getters)]

mod app;
mod platform;
mod stats;
mod sync;
mod system;

#[no_mangle]
pub unsafe extern "C" fn app_main() {
    system::init();
    log::info!("System initialized");

    system::task::top::start();
    stats::start();

    platform::init();
    log::info!("Platform initialized");
}

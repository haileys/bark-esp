#![no_std]
#![no_main]
#![feature(sync_unsafe_cell)]
#![feature(type_alias_impl_trait)]

mod platform;
mod sync;
mod system;

#[no_mangle]
pub unsafe extern "C" fn app_main() {
    system::init();
    log::info!("System initialized");

    platform::init();
    log::info!("Platform initialized");

    system::task::log_tasks();
}

pub mod event;
pub mod eventloop;
pub mod network;
pub mod nvs;
pub mod wifi;

pub async unsafe fn init() {
    eventloop::init();
    nvs::init();
    wifi::init();
    network::init();
}

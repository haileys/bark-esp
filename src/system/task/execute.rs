use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use core::ptr::null_mut;

use esp_idf_sys as sys;

use super::registry::TaskRegistration;
use super::waker::TaskWaker;

pub fn execute<Ret>(fut: impl Future<Output = Ret>) -> Ret {
    futures::pin_mut!(fut);

    let registration = TaskRegistration::new_for_current_task();
    let waker = TaskWaker::new(registration.id()).to_waker();
    let mut cx = Context::from_waker(&waker);

    loop {
        if let Poll::Ready(ret) = Pin::new(&mut fut).poll(&mut cx) {
            return ret;
        }

        unsafe {
            sys::xTaskGenericNotifyWait(
                0,
                0,
                0,
                null_mut(),
                sys::freertos_wait_forever,
            );
        }
    }
}

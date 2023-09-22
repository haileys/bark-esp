use core::future::Future;
use core::task::{Context, Poll};
use core::ptr::null_mut;

use esp_idf_sys as sys;

use super::registry::TaskRegistration;
use super::waker::TaskWaker;

pub fn execute<Func, Fut, Ret>(func: Func) -> Ret
where
    Func: FnOnce() -> Fut,
    Fut: Future<Output = Ret>,
{
    let registration = TaskRegistration::new_for_current_task();
    let waker = TaskWaker::new(registration.id()).to_waker();
    let mut cx = Context::from_waker(&waker);

    let fut = func();
    futures::pin_mut!(fut);

    loop {
        if let Poll::Ready(ret) = fut.as_mut().poll(&mut cx) {
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

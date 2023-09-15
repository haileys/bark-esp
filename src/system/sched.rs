use core::sync::atomic::{AtomicUsize, Ordering};

use esp_idf_sys::{vTaskSuspendAll, xTaskResumeAll};

static SUSPEND_COUNT: AtomicUsize = AtomicUsize::new(0);

pub struct Suspend(());

pub fn suspend() -> Suspend {
    let prev = SUSPEND_COUNT.fetch_add(1, Ordering::SeqCst);

    if prev == 0 {
        unsafe { vTaskSuspendAll(); }
    }

    Suspend(())
}

impl Drop for Suspend {
    fn drop(&mut self) {
        let prev = SUSPEND_COUNT.fetch_sub(1, Ordering::SeqCst);

        if prev == 1 {
            unsafe { xTaskResumeAll(); }
        }
    }
}

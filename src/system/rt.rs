use core::cell::SyncUnsafeCell;

use bitflags::bitflags;
use cstr::cstr;
use embassy_executor::{Executor, _export::StaticCell, SendSpawner};

use super::{task, eventgroup::EventGroup};

static STATE: EventGroup<ExecutorState> = EventGroup::declare();
static EXECUTOR: StaticCell<Executor> = StaticCell::new();
static SPAWNER: SyncUnsafeCell<Option<SendSpawner>> = SyncUnsafeCell::new(None);

bitflags! {
    #[derive(Clone, Copy)]
    struct ExecutorState: u32 {
        const RUNNING = 1 << 0;
    }
}

pub unsafe fn init() {
    STATE.init_with(ExecutorState::empty());

    task::new(cstr!("bark: async-rt"))
        .spawn(executor_task)
        .expect("start async executor task")
}

pub fn spawner() -> SendSpawner {
    STATE.wait_all(ExecutorState::RUNNING);
    let spawner = unsafe { &*SPAWNER.get() };
    spawner.unwrap()
}

fn executor_task() {
    let executor = EXECUTOR.init_with(Executor::new);
    executor.run(|spawner| {
        unsafe {
            core::ptr::write(SPAWNER.get(), Some(spawner.make_send()));
        }
        STATE.set(ExecutorState::RUNNING);
    })
}

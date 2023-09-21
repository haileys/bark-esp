use core::sync::atomic::{AtomicPtr, Ordering};
use core::ptr::{null_mut, NonNull};

use esp_idf_sys as sys;

use crate::system::task::{self, TaskPtr};

pub const MAX_TASKS: usize = 32;

pub struct TaskRegistration {
    id: TaskId,
}

impl TaskRegistration {
    pub fn new_for_current_task() -> Self {
        let task = task::current();

        for id in TaskId::iter() {
            if id.slot().try_claim(task).is_ok() {
                return TaskRegistration { id }
            }
        }

        panic!("failed to register task, all slots taken! there should never be this many tasks!");
    }

    pub fn id(&self) -> TaskId {
        self.id
    }
}

impl Drop for TaskRegistration {
    fn drop(&mut self) {
        self.id.slot().clear();
    }
}

#[repr(transparent)]
#[derive(Copy, Clone, Debug)]
pub struct TaskId(u8);

impl TaskId {
    pub fn new(num: usize) -> Self {
        if num > MAX_TASKS {
            panic!("argument passed to TaskId::new greater than MAX_TASKS");
        }

        TaskId(num as u8)
    }

    pub fn current() -> Self {
        let task = task::current();

        for id in Self::iter() {
            if id.slot().load() == Some(task) {
                return id;
            }
        }

        panic!("must not call TaskId::current from non-registered task");
    }

    pub fn from_opaque_ptr(opaque: *const ()) -> Self {
        TaskId::new(opaque as usize)
    }

    pub fn as_opaque_ptr(self) -> *const () {
        usize::from(self.0) as *const ()
    }

    pub fn slot(&self) -> &'static TaskSlot {
        &SLOTS[usize::from(self.0)]
    }

    pub fn as_bit(&self) -> u32 {
        1 << self.0
    }

    pub fn iter() -> impl Iterator<Item = TaskId> {
        (0..MAX_TASKS).map(TaskId::new)
    }
}

#[repr(transparent)]
pub struct TaskSlot {
    ptr: AtomicPtr<sys::tskTaskControlBlock>
}

impl TaskSlot {
    pub const fn empty() -> Self {
        TaskSlot { ptr: AtomicPtr::new(null_mut()) }
    }

    pub fn load(&self) -> Option<TaskPtr> {
        NonNull::new(self.ptr.load(Ordering::Relaxed))
    }

    fn clear(&self) {
        self.ptr.store(null_mut(), Ordering::Relaxed)
    }

    fn try_claim(&self, task: TaskPtr) -> Result<(), ()> {
        let result = self.ptr.compare_exchange(
            null_mut(),
            task.as_ptr(),
            Ordering::SeqCst,
            Ordering::SeqCst,
        );

        result.map(|_| ()).map_err(|_| ())
    }
}

static SLOTS: [TaskSlot; MAX_TASKS] = [
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
    TaskSlot::empty(),
];

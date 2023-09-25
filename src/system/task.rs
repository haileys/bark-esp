use core::ffi::{CStr, c_void};
use core::fmt::Debug;
use core::future::Future;
use core::ptr::{self, NonNull};

use derive_more::From;
use esp_idf_sys as sys;

use super::heap::{HeapBox, MallocError};

mod execute;
mod registry;
mod waker;
pub mod top;

pub use waker::TaskWakerSet;

pub type TaskPtr = NonNull<sys::tskTaskControlBlock>;

const DEFAULT_STACK_SIZE: u32 = 8192;
const DEFAULT_PRIORITY: u32 = 0;

#[must_use = "must call TaskBuilder::spawn to actually create task"]
pub struct TaskBuilder {
    name: &'static str,
    stack_bytes: u32,
    priority: u32,
    core: i32,
}

pub fn new(name: &'static str) -> TaskBuilder {
    TaskBuilder {
        name,
        stack_bytes: DEFAULT_STACK_SIZE,
        priority: DEFAULT_PRIORITY,
        core: 0,
    }
}

pub fn current() -> TaskPtr {
    unsafe {
        NonNull::new_unchecked(sys::xTaskGetCurrentTaskHandle())
    }
}

#[derive(Debug, From)]
pub enum SpawnError {
    AllocateClosure(MallocError),
    TaskCreateError,
}

impl TaskBuilder {
    #[allow(unused)]
    pub fn stack_size(mut self, bytes: usize) -> Self {
        self.stack_bytes = bytes as u32;
        self
    }

    #[allow(unused)]
    pub fn priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    #[allow(unused)]
    pub fn use_alternate_core(mut self) -> Self {
        self.core = 1;
        self
    }

    pub fn spawn<F, Fut, R>(self, main: F) -> Result<(), SpawnError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = R>,
        R: TaskReturn,
    {
        let boxed_main = HeapBox::alloc(main)?;

        unsafe extern "C" fn start<F, Fut, R>(param: *mut c_void)
        where
            F: FnOnce() -> Fut + Send + 'static,
            Fut: Future<Output = R>,
            R: TaskReturn
        {
            // do all the heavy lifting in a scope to ensure that destructors
            // are run on any left over values before we call vTaskDelete:
            {
                // unbox closure from param
                let boxed_main = NonNull::new_unchecked(param).cast::<F>();
                let boxed_main = HeapBox::from_raw(boxed_main);
                let main = HeapBox::into_inner(boxed_main);

                // invoke closure as task routine
                let result = execute::execute(main);

                // get task name
                let name = CStr::from_ptr(sys::pcTaskGetName(ptr::null_mut()));
                let name = name.to_str().unwrap_or_default();

                // log task exit with task name
                result.log(name);
            }

            // freertos tasks must never return, instead delete current task:
            sys::vTaskDelete(ptr::null_mut());
            unreachable!();
        }

        let boxed_main_ptr = HeapBox::into_raw(boxed_main);

        log::info!("Spawning task: {}", self.name);

        let stack_size = self.stack_bytes + core::mem::size_of::<Fut>() as u32;

        const TASK_NAME_LENGTH: usize = 32;
        let mut name = heapless::Vec::<u8, { TASK_NAME_LENGTH + 1 }>::new();
        let _ = name.extend(self.name.bytes().take(TASK_NAME_LENGTH));
        let _ = name.push(0);

        let rc = unsafe {
            sys::xTaskCreatePinnedToCore(
                Some(start::<F, Fut, R>),
                name.as_ptr().cast(),
                stack_size,
                boxed_main_ptr.cast::<c_void>().as_ptr(),
                self.priority,
                ptr::null_mut(),
                self.core,
            )
        };

        if rc == 1 {
            Ok(())
        } else {
            // if the task failed to spawn, there's nobody to receive
            // ownership of boxed_main, so we need to take it back
            let _boxed_main = unsafe { HeapBox::from_raw(boxed_main_ptr) };

            Err(SpawnError::TaskCreateError)
        }
    }
}

pub trait TaskReturn {
    fn log(self, task_name: &str);
}

impl TaskReturn for () {
    fn log(self, task_name: &str) {
        log::info!("{task_name} exited");
    }
}

impl<T: TaskReturn, E: Debug> TaskReturn for Result<T, E> {
    fn log(self, task_name: &str) {
        match self {
            Ok(val) => val.log(task_name),
            Err(err) => {
                log::error!("{task_name} failed with error: {err:?}");
            }
        }
    }
}

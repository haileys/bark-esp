use core::ffi::{CStr, c_void};
use core::fmt::{Display, self};
use core::ptr::{self, NonNull};

use ascii::AsciiStr;
use derive_more::From;
use esp_idf_sys as sys;
use esp_println::{println, print};
use heapless::Vec;

use super::heap::{HeapBox, MallocError};

const MAX_TASKS: usize = 32;
const DEFAULT_STACK_SIZE: usize = 8192;
const DEFAULT_PRIORITY: u32 = 0;

#[must_use = "must call TaskBuilder::spawn to actually create task"]
pub struct TaskBuilder {
    name: &'static CStr,
    stack_bytes: usize,
    priority: u32,
    core: i32,
}

pub fn new(name: &'static CStr) -> TaskBuilder {
    TaskBuilder {
        name,
        stack_bytes: DEFAULT_STACK_SIZE,
        priority: DEFAULT_PRIORITY,
        core: 0,
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
        self.stack_bytes = bytes;
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

    pub fn spawn<F: FnOnce() + Send + 'static>(self, main: F) -> Result<(), SpawnError> {
        let boxed_main = HeapBox::alloc(main)?;
        let stack_words = self.stack_bytes;// / mem::size_of::<usize>();

        unsafe extern "C" fn start<F: FnOnce() + Send + 'static>(param: *mut c_void) {
            let boxed_main = NonNull::new_unchecked(param).cast::<F>();
            let boxed_main = HeapBox::from_raw(boxed_main);
            let main = HeapBox::into_inner(boxed_main);
            main();
        }

        let boxed_main_ptr = HeapBox::into_raw(boxed_main);

        let rc = unsafe {
            sys::xTaskCreatePinnedToCore(
                Some(start::<F>),
                self.name.as_ptr(),
                stack_words as u32,
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

macro_rules! print_task {
    ( $($expr:tt)* ) => {
        print!("{id: >4}  {state: <7}  {affinity: >3}  {priority: >4}  {name: <16}  {stack: >8}  {stack_headroom: >8}", $($expr)*)
    }
}

pub fn log_tasks() {
    let tasks = get_tasks();

    print!("\x1b[104;30m");
    print_task!(
        id = "ID",
        state = "STATE",
        affinity = "CPU",
        priority = "PRIO",
        name = "NAME",
        stack = "STACK",
        stack_headroom = "HEADROOM",
    );
    println!("\x1b[0m");

    for task in tasks {
        print_task!(
            id = task.id(),
            state = task.state(),
            affinity = task.affinity(),
            priority = task.priority(),
            name = task.name(),
            stack = task.stack(),
            stack_headroom = task.stack_high_watermark(),
        );
        println!();
    }
}

fn get_tasks() -> Vec<TaskStatus, MAX_TASKS> {
    let mut tasks = Vec::new();

    // SAFETY: this is technically unsafe... because we are accessing the name
    // pointer in TaskStatus_t, and if the task gets deleted this could be a
    // use after free. I actually don't know how to make it safe. shrug
    unsafe {
        let ntasks = sys::uxTaskGetSystemState(
            tasks.as_mut_ptr() as *mut sys::TaskStatus_t,
            tasks.capacity() as u32,
            ptr::null_mut(),
        );

        if ntasks == 0 {
            log::warn!("more than MAX_TASKS ({MAX_TASKS}) tasks in system, can't call uxTaskGetSystemState");
        }

        tasks.set_len(ntasks as usize);
    };

    tasks
}

#[repr(transparent)]
struct TaskStatus(sys::TaskStatus_t);

impl TaskStatus {
    pub fn id(&self) -> u32 {
        self.0.xTaskNumber
    }

    pub fn state(&self) -> TaskState {
        self.0.eCurrentState.into()
    }

    pub fn affinity(&self) -> CoreAffinity {
        let core_id = self.0.xCoreID as u32;

        if core_id == sys::tskNO_AFFINITY {
            CoreAffinity::Any
        } else {
            CoreAffinity::Pinned(core_id)
        }
    }

    pub fn priority(&self) -> u32 {
        self.0.uxCurrentPriority
    }

    pub fn stack(&self) -> StackBase {
        StackBase(self.0.pxStackBase)
    }

    pub fn stack_high_watermark(&self) -> usize {
        self.0.usStackHighWaterMark as usize
    }

    pub fn name(&self) -> &AsciiStr {
        let cstr = unsafe { CStr::from_ptr(self.0.pcTaskName) };
        AsciiStr::from_ascii(cstr.to_bytes())
            .unwrap_or_default()
    }
}

struct StackBase(*mut u8);

impl Display for StackBase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let base = self.0 as usize;
        fmt::LowerHex::fmt(&base, f)
    }
}

enum CoreAffinity {
    Any,
    Pinned(u32),
}

impl Display for CoreAffinity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreAffinity::Any => "*".fmt(f),
            CoreAffinity::Pinned(core) => core.fmt(f),
        }
    }
}

impl From<i32> for CoreAffinity {
    fn from(value: i32) -> Self {
        value.try_into()
            .map(CoreAffinity::Pinned)
            .unwrap_or(CoreAffinity::Any)
    }
}

#[derive(Copy, Clone)]
#[repr(u32)]
enum TaskState {
    Ready     = sys::eTaskState_eReady,
    Running   = sys::eTaskState_eRunning,
    Blocked   = sys::eTaskState_eBlocked,
    Suspended = sys::eTaskState_eSuspended,
    Deleted   = sys::eTaskState_eDeleted,
    Invalid   = sys::eTaskState_eInvalid,
}

impl Display for TaskState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let str = match self {
            TaskState::Ready     => "ready",
            TaskState::Running   => "running",
            TaskState::Blocked   => "blocked",
            TaskState::Suspended => "suspend",
            TaskState::Deleted   => "deleted",
            TaskState::Invalid   => "",
        };

        str.fmt(f)
    }
}

impl From<sys::eTaskState> for TaskState {
    fn from(value: sys::eTaskState) -> Self {
        match value {
            sys::eTaskState_eReady     => TaskState::Ready,
            sys::eTaskState_eRunning   => TaskState::Running,
            sys::eTaskState_eBlocked   => TaskState::Blocked,
            sys::eTaskState_eSuspended => TaskState::Suspended,
            sys::eTaskState_eDeleted   => TaskState::Deleted,
            _ => TaskState::Invalid,
        }
    }
}

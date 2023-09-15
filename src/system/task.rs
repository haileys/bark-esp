use core::ffi::CStr;
use core::fmt::{Display, self};
use core::ptr;

use ascii::AsciiStr;
use esp_idf_sys::{TaskStatus_t, uxTaskGetSystemState, eTaskState};
use heapless::Vec;
use esp_println::{println, print};

const MAX_TASKS: usize = 16;

macro_rules! print_task {
    ( $($expr:tt)* ) => {
        print!("{id: >4}  {state: <7}  {affinity: >3}  {priority: >4}  {name: <16}", $($expr)*)
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
    );
    println!("\x1b[0m");

    for task in tasks {
        print_task!(
            id = task.id(),
            state = task.state(),
            affinity = task.affinity(),
            priority = task.priority(),
            name = task.name(),
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
        let ntasks = uxTaskGetSystemState(
            tasks.as_mut_ptr() as *mut TaskStatus_t,
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
struct TaskStatus(TaskStatus_t);

impl TaskStatus {
    pub fn id(&self) -> u32 {
        self.0.xTaskNumber
    }

    pub fn state(&self) -> TaskState {
        self.0.eCurrentState.into()
    }

    pub fn affinity(&self) -> CoreAffinity {
        self.0.xCoreID.into()
    }

    pub fn priority(&self) -> u32 {
        self.0.uxCurrentPriority
    }

    pub fn name(&self) -> &AsciiStr {
        let cstr = unsafe { CStr::from_ptr(self.0.pcTaskName) };
        AsciiStr::from_ascii(cstr.to_bytes())
            .unwrap_or_default()
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
    Ready = esp_idf_sys::eTaskState_eReady,
    Running = esp_idf_sys::eTaskState_eRunning,
    Blocked = esp_idf_sys::eTaskState_eBlocked,
    Suspended = esp_idf_sys::eTaskState_eSuspended,
    Deleted = esp_idf_sys::eTaskState_eDeleted,
    Invalid = esp_idf_sys::eTaskState_eInvalid,
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

impl From<eTaskState> for TaskState {
    fn from(value: eTaskState) -> Self {
        match value {
            esp_idf_sys::eTaskState_eReady => TaskState::Ready,
            esp_idf_sys::eTaskState_eRunning => TaskState::Running,
            esp_idf_sys::eTaskState_eBlocked => TaskState::Blocked,
            esp_idf_sys::eTaskState_eSuspended => TaskState::Suspended,
            esp_idf_sys::eTaskState_eDeleted => TaskState::Deleted,
            _ => TaskState::Invalid,
        }
    }
}

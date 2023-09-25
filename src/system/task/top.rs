use core::ffi::CStr;
use core::fmt::{Display, self};

use ascii::AsciiStr;
use esp_idf_sys as sys;
use esp_println::{println, print};
use heapless::Vec;

const MAX_TOP_TASKS: usize = 32;

pub fn start() {
    // super::new("bark::top")
    //     .priority(0)
    //     .spawn(task)
    //     .unwrap();
}

macro_rules! print_task {
    ( $($expr:tt)* ) => {
        print!("{id: >4}  {state: <7}  {affinity: >3}  {priority: >4}  {name: <16}  {cpu: >4}  {stack: >8}  {stack_headroom: >8}", $($expr)*)
    }
}

#[allow(unused)]
async fn task() {
    let mut prev_state = get_system_state();

    loop {
        unsafe { sys::vTaskDelay(1000); }
        let state = get_system_state();

        // save cursor position
        print!("\x1b[s");
        // move to top left corner
        print!("\x1b[0;0H");

        // render task header:
        print!("\x1b[104;30m");
        print_task!(
            id = "ID",
            state = "STATE",
            name = "NAME",
            cpu = "CPU%",
            affinity = "AFF",
            priority = "PRIO",
            stack = "STACK",
            stack_headroom = "HEADROOM",
        );
        println!("\x1b[0m");

        // print tasks
        for task in &state.tasks {
            let prev_ticks = prev_state.tasks.iter()
                .find(|t| t.id() == task.id())
                .map(|t| t.runtime_tick_counter())
                .unwrap_or_default();

            let elapsed_ticks = state.elapsed_ticks - prev_state.elapsed_ticks;

            let cpu_pct = ((task.runtime_tick_counter() - prev_ticks) * 100) / elapsed_ticks;

            print_task!(
                id = task.id(),
                state = task.state(),
                name = task.name(),
                cpu = cpu_pct,
                affinity = task.affinity(),
                priority = task.priority(),
                stack = task.stack(),
                stack_headroom = task.stack_high_watermark(),
            );
            println!();
        }

        // restore cursor
        print!("\x1b[u");

        prev_state = state;
    }
}

struct SystemState {
    elapsed_ticks: u32,
    tasks: Vec<TaskStatus, MAX_TOP_TASKS>,
}

fn get_system_state() -> SystemState {
    let mut elapsed_ticks = 0;
    let mut tasks = Vec::new();

    // SAFETY: this is technically unsafe... because we are accessing the name
    // pointer in TaskStatus_t, and if the task gets deleted this could be a
    // use after free. I actually don't know how to make it safe. shrug
    unsafe {
        let ntasks = sys::uxTaskGetSystemState(
            tasks.as_mut_ptr() as *mut sys::TaskStatus_t,
            tasks.capacity() as u32,
            &mut elapsed_ticks,
        );

        if ntasks == 0 {
            log::warn!("more than MAX_TASKS ({MAX_TOP_TASKS}) tasks in system, can't call uxTaskGetSystemState");
        }

        tasks.set_len(ntasks as usize);
    };

    SystemState { elapsed_ticks, tasks }
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

    pub fn runtime_tick_counter(&self) -> u32 {
        self.0.ulRunTimeCounter
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
            CoreAffinity::Any => Display::fmt("*", f),
            CoreAffinity::Pinned(core) => Display::fmt(core, f),
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

        Display::fmt(str, f)
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

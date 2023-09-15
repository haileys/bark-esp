macro_rules! precondition {
    ($assert:expr) => {
        if !($assert) {
            debug_assert!($assert);
            return;
        }
    }
}

pub(crate) use precondition;

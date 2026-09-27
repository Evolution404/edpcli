//! One launch boundary decides whether external task execution exists.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionPolicy {
    Live,
    DemoNoExternalIo,
}

impl ExecutionPolicy {
    pub const fn permits_external_io(self) -> bool {
        matches!(self, Self::Live)
    }

    /// The live loop owns every external task service; demo receives no such service.
    pub fn run_external<T>(self, operation: impl FnOnce() -> T) -> Option<T> {
        self.permits_external_io().then(operation)
    }
}

pub fn launch(policy: ExecutionPolicy, scene: &str) -> i32 {
    match policy {
        ExecutionPolicy::Live => policy
            .run_external(super::run)
            .unwrap_or(crate::common::EXIT_IO),
        ExecutionPolicy::DemoNoExternalIo => super::demo::run_interactive(scene),
    }
}

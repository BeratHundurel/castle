use gpui::Task;

#[derive(Default)]
pub(crate) struct RequestTracker {
    generation: u64,
    task: Option<Task<()>>,
}

impl RequestTracker {
    pub(crate) fn with_task(generation: u64, task: Task<()>) -> Self {
        Self {
            generation,
            task: Some(task),
        }
    }

    pub(crate) fn begin(&mut self) -> u64 {
        self.task = None;
        self.generation = self.generation.saturating_add(1);
        self.generation
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn set_task(&mut self, task: Task<()>) {
        self.task = Some(task);
    }

    pub(crate) fn clear(&mut self) {
        self.task = None;
    }
}

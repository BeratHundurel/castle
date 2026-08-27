use gpui::Task;

#[derive(Default)]
pub struct RequestTracker {
    generation: u64,
    task: Option<Task<()>>,
}

impl RequestTracker {
    pub fn with_task(generation: u64, task: Task<()>) -> Self {
        Self {
            generation,
            task: Some(task),
        }
    }

    pub fn begin(&mut self) -> u64 {
        self.task = None;
        self.generation = self.generation.saturating_add(1);
        self.generation
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn set_task(&mut self, task: Task<()>) {
        self.task = Some(task);
    }

    pub fn clear(&mut self) {
        self.task = None;
    }
}

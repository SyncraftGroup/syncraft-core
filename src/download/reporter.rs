use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ProgressState {
    pub stage: String,
    pub current: u64,
    pub total: u64,
}

pub trait ProgressReporter: Send + Sync {
    fn on_progress(&self, state: ProgressState);
}

impl ProgressReporter for Arc<dyn ProgressReporter> {
    fn on_progress(&self, state: ProgressState) {
        self.as_ref().on_progress(state)
    }
}

impl ProgressReporter for Box<dyn ProgressReporter> {
    fn on_progress(&self, state: ProgressState) {
        self.as_ref().on_progress(state)
    }
}

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::progress::Progress;

const OVERALL_TEMPLATE: &str = "{prefix} [{bar:40.cyan/blue}] {pos}/{len}";
const FILE_TEMPLATE: &str = "  {msg:.40} [{bar:30.green}] {bytes:>10}/{total_bytes:>10} ({bytes_per_sec:>10}) ETA: {eta:>4}";
const SPINNER_TEMPLATE: &str = "{spinner:.cyan} {msg}";
const PROGRESS_CHARS: &str = "=>-";

/// Terminal progress bars. See [`Progress`] for the sink used by core code.
pub struct UiProgress {
    mp: MultiProgress,
    overall: ProgressBar,
    /// Bars for in-flight files, keyed by the name passed to [`Progress`].
    files: Mutex<HashMap<String, ProgressBar>>,
    /// The current indeterminate step, if any.
    stage: Mutex<Option<ProgressBar>>,
}

impl UiProgress {
    pub fn new(total: u64) -> Self {
        Self::with_prefix(total, "Files")
    }

    pub fn empty() -> Self {
        Self::new(0)
    }

    pub fn with_prefix(total: u64, prefix: &str) -> Self {
        let mp = MultiProgress::new();
        let style = ProgressStyle::with_template(OVERALL_TEMPLATE)
            .unwrap()
            .progress_chars(PROGRESS_CHARS);
        let overall = mp.add(ProgressBar::new(total));
        overall.set_style(style);
        overall.set_prefix(prefix.to_string());
        Self {
            mp,
            overall,
            files: Mutex::new(HashMap::new()),
            stage: Mutex::new(None),
        }
    }
}

impl Progress for UiProgress {
    fn set_total(&self, total: u64) {
        self.overall.set_length(total);
    }

    fn add_total(&self, n: u64) {
        self.overall.inc_length(n);
    }

    fn overall_inc(&self) {
        self.overall.inc(1);
    }

    fn overall_finish(&self, msg: &str) {
        self.overall.set_prefix(msg.to_string());
        self.overall.finish();
    }

    fn spinner(&self, msg: &str) {
        self.spinner_done(msg);
        let style = ProgressStyle::with_template(SPINNER_TEMPLATE).unwrap();
        let pb = self.mp.add(ProgressBar::new_spinner());
        pb.set_style(style);
        pb.set_message(msg.to_string());
        pb.enable_steady_tick(Duration::from_millis(80));
        *self.stage.lock().unwrap() = Some(pb);
    }

    fn spinner_done(&self, _msg: &str) {
        if let Some(pb) = self.stage.lock().unwrap().take() {
            pb.finish_and_clear();
        }
    }

    fn file_start(&self, name: &str, size: u64) {
        self.file_done(name);
        let style = ProgressStyle::with_template(FILE_TEMPLATE)
            .unwrap()
            .progress_chars(PROGRESS_CHARS);
        let pb = self.mp.add(ProgressBar::new(size));
        pb.set_style(style);
        pb.set_message(name.to_string());
        self.files.lock().unwrap().insert(name.to_string(), pb);
    }

    fn file_total(&self, name: &str, total: u64) {
        if let Some(pb) = self.files.lock().unwrap().get(name) {
            pb.set_length(total);
        }
    }

    fn file_inc(&self, name: &str, bytes: u64) {
        if let Some(pb) = self.files.lock().unwrap().get(name) {
            pb.inc(bytes);
        }
    }

    fn file_done(&self, name: &str) {
        if let Some(pb) = self.files.lock().unwrap().remove(name) {
            pb.finish_and_clear();
        }
    }
}

use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

const OVERALL_TEMPLATE: &str = "{prefix} [{bar:40.cyan/blue}] {pos}/{len}";
const FILE_TEMPLATE: &str = "  {msg:.40} [{bar:30.green}] {bytes:>10}/{total_bytes:>10} ({bytes_per_sec:>10}) ETA: {eta:>4}";
const SPINNER_TEMPLATE: &str = "{spinner:.cyan} {msg}";
const PROGRESS_CHARS: &str = "=>-";

pub struct UiProgress {
    mp: MultiProgress,
    overall: ProgressBar,
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
        Self { mp, overall }
    }

    pub fn set_total(&self, total: u64) {
        self.overall.set_length(total);
    }

    pub fn add_total(&self, n: u64) {
        self.overall.inc_length(n);
    }

    pub fn overall_inc(&self) {
        self.overall.inc(1);
    }

    pub fn overall_finish(&self, msg: &str) {
        self.overall.set_prefix(msg.to_string());
        self.overall.finish();
    }

    pub fn add_download_bar(&self, name: &str, size: u64) -> ProgressBar {
        let style = ProgressStyle::with_template(FILE_TEMPLATE)
            .unwrap()
            .progress_chars(PROGRESS_CHARS);
        let pb = self.mp.add(ProgressBar::new(size));
        pb.set_style(style);
        pb.set_message(name.to_string());
        pb
    }

    pub fn add_spinner(&self, msg: &str) -> ProgressBar {
        let style = ProgressStyle::with_template(SPINNER_TEMPLATE).unwrap();
        let pb = self.mp.add(ProgressBar::new_spinner());
        pb.set_style(style);
        pb.set_message(msg.to_string());
        pb.enable_steady_tick(Duration::from_millis(80));
        pb
    }

    pub fn finish_bar(&self, pb: &ProgressBar) {
        pb.finish_and_clear();
    }

    pub fn finish_spinner(&self, pb: &ProgressBar, msg: &str) {
        pb.set_style(ProgressStyle::with_template("  ✓ {msg}").unwrap());
        pb.finish_with_message(msg.to_string());
    }
}

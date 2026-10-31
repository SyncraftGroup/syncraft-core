//! Progress reporting for long operations.
//!
//! Core code used to depend on [`indicatif`] directly, which meant a GUI had no
//! way to show what was happening. Operations now talk to this trait; the
//! terminal implementation lives in [`crate::download::UiProgress`], and the
//! desktop app provides its own sink that forwards into the UI thread.

/// A sink for install/download progress.
///
/// Every method has a no-op default so a caller only has to implement what it
/// cares about. Implementations must be cheap and non-blocking: they are called
/// from async download tasks.
pub trait Progress: Send + Sync {
    /// Total number of files for the current phase.
    fn set_total(&self, _total: u64) {}

    /// Grow the total (used when more files are discovered mid-flight).
    fn add_total(&self, _n: u64) {}

    /// One file finished.
    fn overall_inc(&self) {}

    /// The phase finished.
    fn overall_finish(&self, _msg: &str) {}

    /// Start an indeterminate step ("Fetching version manifest...").
    fn spinner(&self, _msg: &str) {}

    /// The current indeterminate step finished.
    fn spinner_done(&self, _msg: &str) {}

    /// A file download started. `size` may be 0 when unknown.
    fn file_start(&self, _name: &str, _size: u64) {}

    /// The exact size became known (HTTP `Content-Length`).
    fn file_total(&self, _name: &str, _total: u64) {}

    /// Bytes arrived for a file. Additive.
    fn file_inc(&self, _name: &str, _bytes: u64) {}

    /// A file download finished (successfully or not).
    fn file_done(&self, _name: &str) {}
}

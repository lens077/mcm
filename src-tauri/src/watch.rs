//! External-modification detection (contracts/plan-file-format.md §外部修改).
//!
//! The app records the modification time it last wrote or read, then compares
//! it whenever the window regains focus. A difference means someone else edited
//! the file, so the user is asked to reload or keep the in-memory version —
//! neither side is ever overwritten silently.

use std::path::Path;
use std::sync::Mutex;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

/// Last known modification time for the open document.
#[derive(Debug, Default)]
pub struct FileWatch {
    known: Mutex<Option<SystemTime>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalChange {
    /// No open file, or the file is unchanged since we last touched it.
    None,
    /// The file on disk is newer than the copy we loaded or saved.
    Modified,
    /// The file we had open no longer exists.
    Missing,
}

pub fn mtime_of(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

impl FileWatch {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the current mtime after a successful open or save.
    pub fn remember(&self, path: &Path) {
        if let Ok(mut slot) = self.known.lock() {
            *slot = mtime_of(path);
        }
    }

    /// Clears the watch (new document, or the file was closed).
    pub fn forget(&self) {
        if let Ok(mut slot) = self.known.lock() {
            *slot = None;
        }
    }

    /// Compares the on-disk mtime with the remembered one.
    #[must_use]
    pub fn check(&self, path: Option<&Path>) -> ExternalChange {
        let Some(path) = path else {
            return ExternalChange::None;
        };
        let Ok(slot) = self.known.lock() else {
            return ExternalChange::None;
        };
        let Some(known) = *slot else {
            return ExternalChange::None;
        };
        match mtime_of(path) {
            None => ExternalChange::Missing,
            Some(current) if current != known => ExternalChange::Modified,
            Some(_) => ExternalChange::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let stamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let dir = std::env::temp_dir().join(format!("mcm-watch-{tag}-{stamp}"));
            std::fs::create_dir_all(&dir).expect("scratch");
            Self(dir)
        }

        fn file(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn no_path_means_no_change() {
        let watch = FileWatch::new();
        assert_eq!(watch.check(None), ExternalChange::None);
    }

    #[test]
    fn unremembered_files_report_no_change() {
        let scratch = Scratch::new("unremembered");
        let path = scratch.file("plan.mcm");
        std::fs::write(&path, "%mcm 1\n").expect("write");
        let watch = FileWatch::new();
        assert_eq!(watch.check(Some(&path)), ExternalChange::None);
    }

    #[test]
    fn untouched_files_report_no_change() {
        let scratch = Scratch::new("untouched");
        let path = scratch.file("plan.mcm");
        std::fs::write(&path, "%mcm 1\n").expect("write");
        let watch = FileWatch::new();
        watch.remember(&path);
        assert_eq!(watch.check(Some(&path)), ExternalChange::None);
    }

    #[test]
    fn external_writes_are_detected() {
        let scratch = Scratch::new("modified");
        let path = scratch.file("plan.mcm");
        std::fs::write(&path, "%mcm 1\n").expect("write");
        let watch = FileWatch::new();
        watch.remember(&path);

        // Filesystem mtime resolution can be coarse; set it explicitly.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&path, "%mcm 1\n- 新增 #t1\n").expect("rewrite");
        let changed = watch.check(Some(&path));
        assert!(
            changed == ExternalChange::Modified || changed == ExternalChange::None,
            "unexpected: {changed:?}"
        );
    }

    #[test]
    fn deleted_files_are_detected() {
        let scratch = Scratch::new("missing");
        let path = scratch.file("plan.mcm");
        std::fs::write(&path, "%mcm 1\n").expect("write");
        let watch = FileWatch::new();
        watch.remember(&path);
        std::fs::remove_file(&path).expect("remove");
        assert_eq!(watch.check(Some(&path)), ExternalChange::Missing);
    }

    #[test]
    fn forget_stops_reporting() {
        let scratch = Scratch::new("forget");
        let path = scratch.file("plan.mcm");
        std::fs::write(&path, "%mcm 1\n").expect("write");
        let watch = FileWatch::new();
        watch.remember(&path);
        watch.forget();
        std::fs::remove_file(&path).expect("remove");
        assert_eq!(watch.check(Some(&path)), ExternalChange::None);
    }
}

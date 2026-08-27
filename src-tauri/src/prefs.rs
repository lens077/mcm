//! App preferences: theme, recent files and per-file view state.
//!
//! These live in the app data directory, never inside `.mcm`, so plan files
//! stay pure content and diff cleanly (contracts/plan-file-format.md §视图状态).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const MAX_RECENT: usize = 10;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    pub theme: Option<String>,
    /// Most recently opened paths, newest first.
    #[serde(default)]
    pub recent_files: Vec<String>,
    /// Per-file UI state (last view, zoom, collapsed nodes), keyed by path.
    #[serde(default)]
    pub view_state: BTreeMap<String, serde_json::Value>,
}

impl Prefs {
    /// Moves `path` to the front of the recent list, de-duplicated and capped.
    pub fn touch_recent(&mut self, path: &str) {
        self.recent_files.retain(|entry| entry != path);
        self.recent_files.insert(0, path.to_owned());
        self.recent_files.truncate(MAX_RECENT);
    }

    /// Drops entries whose files no longer exist.
    pub fn prune_missing(&mut self) {
        self.recent_files.retain(|entry| Path::new(entry).exists());
    }
}

/// `<app data>/prefs.json`, created on demand.
#[must_use]
pub fn prefs_path(base: &Path) -> PathBuf {
    base.join("prefs.json")
}

pub fn load(base: &Path) -> Prefs {
    let path = prefs_path(base);
    // Corrupt or missing preferences must never block startup.
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(base: &Path, prefs: &Prefs) -> std::io::Result<()> {
    std::fs::create_dir_all(base)?;
    let text = serde_json::to_string_pretty(prefs)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    std::fs::write(prefs_path(base), text)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let dir = std::env::temp_dir().join(format!("mcm-prefs-{tag}-{stamp}"));
            std::fs::create_dir_all(&dir).expect("scratch");
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn missing_preferences_fall_back_to_defaults() {
        let scratch = Scratch::new("missing");
        let prefs = load(&scratch.0);
        assert_eq!(prefs, Prefs::default());
    }

    #[test]
    fn corrupt_preferences_fall_back_to_defaults() {
        let scratch = Scratch::new("corrupt");
        std::fs::write(prefs_path(&scratch.0), "{ not json").expect("write");
        assert_eq!(load(&scratch.0), Prefs::default());
    }

    #[test]
    fn preferences_round_trip() {
        let scratch = Scratch::new("roundtrip");
        let mut prefs = Prefs {
            theme: Some("dark".into()),
            ..Prefs::default()
        };
        prefs.touch_recent("/tmp/a.mcm");
        prefs.view_state.insert(
            "/tmp/a.mcm".into(),
            serde_json::json!({ "view": "timeline" }),
        );

        save(&scratch.0, &prefs).expect("save");
        assert_eq!(load(&scratch.0), prefs);
    }

    #[test]
    fn recent_files_are_newest_first_and_deduplicated() {
        let mut prefs = Prefs::default();
        prefs.touch_recent("/a.mcm");
        prefs.touch_recent("/b.mcm");
        prefs.touch_recent("/a.mcm");
        assert_eq!(prefs.recent_files, vec!["/a.mcm", "/b.mcm"]);
    }

    #[test]
    fn recent_files_are_capped() {
        let mut prefs = Prefs::default();
        for index in 0..(MAX_RECENT + 5) {
            prefs.touch_recent(&format!("/plan-{index}.mcm"));
        }
        assert_eq!(prefs.recent_files.len(), MAX_RECENT);
        assert_eq!(
            prefs.recent_files[0],
            format!("/plan-{}.mcm", MAX_RECENT + 4)
        );
    }

    #[test]
    fn pruning_drops_files_that_no_longer_exist() {
        let scratch = Scratch::new("prune");
        let existing = scratch.0.join("here.mcm");
        std::fs::write(&existing, "%mcm 1\n").expect("write");

        let mut prefs = Prefs::default();
        prefs.touch_recent("/definitely/not/here.mcm");
        prefs.touch_recent(&existing.display().to_string());
        prefs.prune_missing();

        assert_eq!(prefs.recent_files, vec![existing.display().to_string()]);
    }
}

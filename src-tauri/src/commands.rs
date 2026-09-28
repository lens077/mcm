// `ViewKind`/`ApplyResult` are the frozen IPC contract shapes; the commands
// that return them land in T020 (US1) and T036 (US3).
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::ocr_models;
use crate::prefs::{self, Prefs};
use crate::watch::{ExternalChange, FileWatch};
use mcm_core::edit::EditCommand;
use mcm_core::scene::SceneGraph;
use mcm_core::session::SearchMatch;
use mcm_core::{Session, SessionError, SessionState, ValidationIssue};
use mcm_export::{ExportFormat, ExportReport};
use mcm_import::{GraphSpec, OcrEngine, OcrModel};
use serde::{Deserialize, Serialize};

/// Error envelope shared by every command (contracts/ipc-commands.md §通用约定).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl CommandError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            details: None,
        }
    }

    #[must_use]
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

impl From<SessionError> for CommandError {
    fn from(error: SessionError) -> Self {
        CommandError::new(error.code(), error.to_string())
    }
}

/// Views whose scene graph can go stale after a mutation. Re-exported from the
/// core so the IPC contract and the projection can never drift apart.
pub use mcm_core::scene::ViewKind;

#[must_use]
pub fn all_views() -> Vec<ViewKind> {
    vec![
        ViewKind::Wbs,
        ViewKind::DepGraph,
        ViewKind::Timeline,
        ViewKind::Milestones,
    ]
}

/// Result of every mutating command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyResult {
    pub revision: u64,
    pub issues: Vec<ValidationIssue>,
    pub dirty: bool,
    pub scene_stale: Vec<ViewKind>,
    pub undo_depth: usize,
    pub redo_depth: usize,
}

impl ApplyResult {
    pub fn from_session(session: &Session, scene_stale: Vec<ViewKind>) -> Self {
        Self {
            revision: session.revision(),
            issues: session.issues().to_vec(),
            dirty: session.is_dirty(),
            scene_stale,
            undo_depth: session.undo_depth(),
            redo_depth: session.redo_depth(),
        }
    }
}

/// Managed application state: the authoritative session behind a mutex.
#[derive(Default)]
pub struct AppState {
    pub session: Mutex<Session>,
    pub watch: FileWatch,
    /// App data directory holding `prefs.json`.
    pub data_dir: Mutex<PathBuf>,
    /// Accurate OCR engine, loaded (and digest-verified) on first use.
    pub accurate_ocr: Mutex<Option<Arc<OcrEngine>>>,
    /// Set while the accurate model is being downloaded.
    pub downloading: AtomicBool,
}

impl AppState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            session: Mutex::new(Session::new()),
            watch: FileWatch::new(),
            data_dir: Mutex::new(default_data_dir()),
            accurate_ocr: Mutex::new(None),
            downloading: AtomicBool::new(false),
        }
    }

    fn accurate_engine(&self) -> CommandResult<Arc<OcrEngine>> {
        let mut cached = self
            .accurate_ocr
            .lock()
            .map_err(|_| CommandError::new("E_INTERNAL", "OCR 状态已损坏，请重启应用"))?;
        if let Some(engine) = cached.as_ref() {
            return Ok(Arc::clone(engine));
        }
        let dir = ocr_models::accurate_dir(&self.data_dir());
        let engine = Arc::new(
            OcrEngine::accurate(&dir)
                .map_err(|error| CommandError::new("E_MODEL_MISSING", error.to_string()))?,
        );
        *cached = Some(Arc::clone(&engine));
        Ok(engine)
    }

    fn ocr_status(&self) -> OcrModelStatus {
        let dir = ocr_models::accurate_dir(&self.data_dir());
        OcrModelStatus {
            accurate_installed: mcm_import::models::accurate_installed(&dir),
            accurate_download_bytes: mcm_import::models::accurate_download_size(),
            accurate_dir: dir.display().to_string(),
            downloading: self.downloading.load(Ordering::SeqCst),
        }
    }

    fn data_dir(&self) -> PathBuf {
        self.data_dir
            .lock()
            .map(|dir| dir.clone())
            .unwrap_or_else(|_| default_data_dir())
    }

    /// Locks the session, converting poisoning into an internal error.
    pub fn with_session<R>(
        &self,
        action: impl FnOnce(&mut Session) -> Result<R, CommandError>,
    ) -> Result<R, CommandError> {
        let mut guard = self
            .session
            .lock()
            .map_err(|_| CommandError::new("E_INTERNAL", "会话状态已损坏，请重启应用"))?;
        action(&mut guard)
    }
}

pub type CommandResult<T> = Result<T, CommandError>;

#[tauri::command]
pub fn session_new(state: tauri::State<'_, AppState>) -> CommandResult<SessionState> {
    // A fresh document is not backed by a file yet.
    state.watch.forget();
    state.with_session(|session| {
        *session = Session::new();
        Ok(session.state(session.undo_depth(), session.redo_depth()))
    })
}

#[tauri::command]
pub fn session_state(state: tauri::State<'_, AppState>) -> CommandResult<SessionState> {
    state.with_session(|session| Ok(session.state(session.undo_depth(), session.redo_depth())))
}

#[tauri::command]
pub fn issues_get(state: tauri::State<'_, AppState>) -> CommandResult<Vec<ValidationIssue>> {
    state.with_session(|session| Ok(session.issues().to_vec()))
}

#[tauri::command]
pub fn app_close_check(state: tauri::State<'_, AppState>) -> CommandResult<serde_json::Value> {
    state.with_session(|session| Ok(serde_json::json!({ "dirty": session.is_dirty() })))
}

#[tauri::command]
pub fn outline_text_get(state: tauri::State<'_, AppState>) -> CommandResult<OutlineText> {
    state.with_session(|session| {
        Ok(OutlineText {
            text: session.outline_text(),
        })
    })
}

#[tauri::command]
pub fn outline_text_apply(
    state: tauri::State<'_, AppState>,
    text: String,
) -> CommandResult<ApplyResult> {
    state.with_session(|session| {
        session.apply_outline_text(&text);
        // A full reparse invalidates every view.
        Ok(ApplyResult::from_session(session, all_views()))
    })
}

#[tauri::command]
pub fn scene_get(state: tauri::State<'_, AppState>, view: ViewKind) -> CommandResult<SceneGraph> {
    state.with_session(|session| Ok(session.scene(view)))
}

#[tauri::command]
pub fn search(state: tauri::State<'_, AppState>, query: String) -> CommandResult<SearchResults> {
    state.with_session(|session| {
        Ok(SearchResults {
            matches: session.search(&query),
        })
    })
}

#[tauri::command]
pub fn edit_apply(
    state: tauri::State<'_, AppState>,
    command: EditCommand,
) -> CommandResult<ApplyResult> {
    state.with_session(|session| {
        let stale = session
            .edit(&command)
            .map_err(|error| CommandError::new("E_BAD_TARGET", error.to_string()))?;
        Ok(ApplyResult::from_session(session, stale))
    })
}

#[tauri::command]
pub fn undo(state: tauri::State<'_, AppState>) -> CommandResult<ApplyResult> {
    state.with_session(|session| {
        // An empty stack is a no-op, not an error.
        let stale = session.undo().unwrap_or_default();
        Ok(ApplyResult::from_session(session, stale))
    })
}

#[tauri::command]
pub fn redo(state: tauri::State<'_, AppState>) -> CommandResult<ApplyResult> {
    state.with_session(|session| {
        let stale = session.redo().unwrap_or_default();
        Ok(ApplyResult::from_session(session, stale))
    })
}

#[tauri::command]
pub fn session_open(
    state: tauri::State<'_, AppState>,
    path: String,
) -> CommandResult<SessionState> {
    let target = PathBuf::from(&path);
    let result = state.with_session(|session| {
        session.open(&target)?;
        Ok(session.state(session.undo_depth(), session.redo_depth()))
    })?;
    state.watch.remember(&target);
    remember_recent(&state, &path);
    Ok(result)
}

#[tauri::command]
pub fn session_save(
    state: tauri::State<'_, AppState>,
    path: Option<String>,
) -> CommandResult<SaveResult> {
    let explicit = path.as_ref().map(PathBuf::from);
    let saved = state.with_session(|session| {
        let target = session.save(explicit.as_deref())?;
        Ok(target)
    })?;
    state.watch.remember(&saved);
    let display = saved.display().to_string();
    remember_recent(&state, &display);
    Ok(SaveResult {
        path: display,
        saved: true,
    })
}

#[tauri::command]
pub fn file_check_external(state: tauri::State<'_, AppState>) -> CommandResult<ExternalCheck> {
    let path = state.with_session(|session| Ok(session.path().map(|p| p.to_path_buf())))?;
    let status = state.watch.check(path.as_deref());
    Ok(ExternalCheck {
        status,
        path: path.map(|p| p.display().to_string()),
    })
}

#[tauri::command]
pub fn prefs_get(state: tauri::State<'_, AppState>) -> CommandResult<Prefs> {
    let mut loaded = prefs::load(&state.data_dir());
    loaded.prune_missing();
    Ok(loaded)
}

#[tauri::command]
pub fn prefs_set(state: tauri::State<'_, AppState>, prefs: Prefs) -> CommandResult<Prefs> {
    let dir = state.data_dir();
    prefs::save(&dir, &prefs)
        .map_err(|error| CommandError::new("E_FILE_IO", format!("无法保存偏好：{error}")))?;
    Ok(prefs)
}

/// Adds `path` to the recent list and persists it.
fn remember_recent(state: &tauri::State<'_, AppState>, path: &str) {
    let dir = state.data_dir();
    let mut loaded = prefs::load(&dir);
    loaded.touch_recent(path);
    let _ = prefs::save(&dir, &loaded);
}

fn default_data_dir() -> PathBuf {
    // Overridable for tests; falls back to the platform config directory.
    if let Ok(explicit) = std::env::var("MCM_DATA_DIR") {
        return PathBuf::from(explicit);
    }
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|home| Path::new(&home).join(".config")))
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("mcm")
}

#[tauri::command]
pub fn export_precheck(state: tauri::State<'_, AppState>) -> CommandResult<ExportPrecheck> {
    state.with_session(|session| {
        Ok(ExportPrecheck {
            ok: session.error_count() == 0,
            error_count: session.error_count(),
        })
    })
}

#[tauri::command]
pub fn export_run(
    state: tauri::State<'_, AppState>,
    format: ExportFormat,
    path: String,
) -> CommandResult<ExportReport> {
    let target = PathBuf::from(&path);
    state.with_session(|session| {
        let mut report = match format {
            ExportFormat::Xmind => mcm_export::xmind::export(session.plan(), &target)
                .map_err(|error| CommandError::new(error.code(), error.to_string()))?,
            ExportFormat::Vsdx => mcm_export::vsdx::export(session.plan(), &target)
                .map_err(|error| CommandError::new(error.code(), error.to_string()))?,
        };
        // Exporting with unresolved errors is allowed but always flagged.
        let errors = session.error_count();
        if errors > 0 {
            report.warn(format!("规划仍有 {errors} 个校验错误，导出内容可能不完整"));
        }
        Ok(report)
    })
}

/// Import a diagram — an image (PNG/JPEG/WebP/BMP) or archify HTML — as
/// outline text.
///
/// Runs off the UI thread (OCR takes a few hundred ms) and does not touch the
/// session: the caller decides whether to load the outline into a new plan.
#[tauri::command(async)]
pub fn diagram_import(
    state: tauri::State<'_, AppState>,
    path: String,
    model: Option<OcrModel>,
) -> CommandResult<DiagramImport> {
    import_file(Path::new(&path), model.unwrap_or_default(), || {
        state.accurate_engine()
    })
}

/// `accurate` is only called for images with the accurate model selected,
/// so HTML imports never load (or require) the downloaded model.
fn import_file(
    path: &Path,
    model: OcrModel,
    accurate: impl FnOnce() -> CommandResult<Arc<OcrEngine>>,
) -> CommandResult<DiagramImport> {
    let bytes = std::fs::read(path).map_err(|error| {
        CommandError::new("E_FILE_IO", format!("无法读取 {}：{error}", path.display()))
    })?;
    let title = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let is_html = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm"));
    let imported = if is_html {
        mcm_import::import_html(&bytes, &title)
    } else if model == OcrModel::Accurate {
        mcm_import::import_image_with(&bytes, &title, accurate()?.as_ref())
    } else {
        mcm_import::import_image(&bytes, &title)
    }
    .map_err(|error| CommandError::new("E_IMPORT", error.to_string()))?;
    Ok(DiagramImport {
        outline: imported.outline,
        report: imported.report,
        elapsed_ms: imported.elapsed_ms,
    })
}

/// Extensions whose text the Mermaid importer may read.
const MERMAID_SOURCES: [&str; 5] = ["md", "markdown", "mdx", "mmd", "mermaid"];
/// Mermaid sources are hand-written text; anything larger is not one.
const MERMAID_SOURCE_LIMIT: u64 = 4 * 1024 * 1024;

/// Read a Markdown / MDX / Mermaid file picked in the import dialog, so the webview
/// can parse and render it with the mermaid library. Only those extensions
/// are served; this is not a general file reader.
#[tauri::command(async)]
pub fn diagram_source_read(path: String) -> CommandResult<DiagramSource> {
    read_diagram_source(Path::new(&path))
}

fn read_diagram_source(path: &Path) -> CommandResult<DiagramSource> {
    let allowed = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| MERMAID_SOURCES.iter().any(|e| ext.eq_ignore_ascii_case(e)));
    if !allowed {
        return Err(CommandError::new(
            "E_IMPORT",
            "只能读取 .md / .markdown / .mdx / .mmd / .mermaid 文件",
        ));
    }
    let io = |error: std::io::Error| {
        CommandError::new("E_FILE_IO", format!("无法读取 {}：{error}", path.display()))
    };
    if std::fs::metadata(path).map_err(io)?.len() > MERMAID_SOURCE_LIMIT {
        return Err(CommandError::new(
            "E_IMPORT",
            "文件超过 4 MB，不像是 Markdown、MDX 或 Mermaid 源文件",
        ));
    }
    let bytes = std::fs::read(path).map_err(io)?;
    let text = String::from_utf8(bytes)
        .map_err(|_| CommandError::new("E_IMPORT", "文件不是 UTF-8 文本"))?;
    Ok(DiagramSource {
        text,
        name: path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default(),
    })
}

/// Import a graph the webview already parsed (Mermaid) as outline text.
/// Like `diagram_import`, it does not touch the session.
#[tauri::command(async)]
pub fn graph_import(spec: GraphSpec, fallback_title: String) -> CommandResult<DiagramImport> {
    let imported = mcm_import::import_graph(&spec, &fallback_title)
        .map_err(|error| CommandError::new("E_IMPORT", error.to_string()))?;
    Ok(DiagramImport {
        outline: imported.outline,
        report: imported.report,
        elapsed_ms: imported.elapsed_ms,
    })
}

/// Whether the accurate OCR model is installed, and where.
#[tauri::command]
pub fn ocr_model_status(state: tauri::State<'_, AppState>) -> CommandResult<OcrModelStatus> {
    Ok(state.ocr_status())
}

/// Download the accurate OCR model (explicit user action only). Emits
/// `ocr-model-download` events with `{done, total}` bytes.
#[tauri::command(async)]
pub fn ocr_model_download(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> CommandResult<OcrModelStatus> {
    use tauri::Emitter;

    if state.downloading.swap(true, Ordering::SeqCst) {
        return Err(CommandError::new("E_BUSY", "高精度模型正在下载中"));
    }
    let dir = ocr_models::accurate_dir(&state.data_dir());
    let mut last_percent = u64::MAX;
    let result = ocr_models::download(
        &dir,
        &mcm_import::models::ACCURATE_FILES,
        &mcm_import::models::ACCURATE_MIRRORS,
        |done, total| {
            // One event per percent is plenty for a progress bar.
            let percent = done * 100 / total.max(1);
            if percent != last_percent {
                last_percent = percent;
                let _ = app.emit(
                    "ocr-model-download",
                    serde_json::json!({ "done": done, "total": total }),
                );
            }
        },
    );
    state.downloading.store(false, Ordering::SeqCst);
    result.map_err(|message| CommandError::new("E_DOWNLOAD", message))?;
    Ok(state.ocr_status())
}

/// Delete the downloaded accurate model to free disk space.
#[tauri::command]
pub fn ocr_model_remove(state: tauri::State<'_, AppState>) -> CommandResult<OcrModelStatus> {
    if state.downloading.load(Ordering::SeqCst) {
        return Err(CommandError::new("E_BUSY", "高精度模型正在下载中"));
    }
    if let Ok(mut cached) = state.accurate_ocr.lock() {
        *cached = None;
    }
    let dir = ocr_models::accurate_dir(&state.data_dir());
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|error| {
            CommandError::new("E_FILE_IO", format!("无法删除 {}：{error}", dir.display()))
        })?;
    }
    Ok(state.ocr_status())
}

/// Payload for `ocr_model_status` / `ocr_model_download` / `ocr_model_remove`.
#[derive(Debug, Clone, Serialize)]
pub struct OcrModelStatus {
    pub accurate_installed: bool,
    pub accurate_download_bytes: u64,
    /// Where the model files live; users without network can copy them here.
    pub accurate_dir: String,
    pub downloading: bool,
}

/// Payload for `diagram_source_read`.
#[derive(Debug, Clone, Serialize)]
pub struct DiagramSource {
    pub text: String,
    /// File name without extension, the fallback plan title.
    pub name: String,
}

/// Payload for `diagram_import`.
#[derive(Debug, Clone, Serialize)]
pub struct DiagramImport {
    pub outline: String,
    pub report: mcm_import::ImportReport,
    pub elapsed_ms: u64,
}

/// Payload for `export_precheck`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportPrecheck {
    pub ok: bool,
    pub error_count: usize,
}

/// Payload for `session_save`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveResult {
    pub path: String,
    pub saved: bool,
}

/// Payload for `file_check_external`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalCheck {
    pub status: ExternalChange,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Payload for `outline_text_get`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineText {
    pub text: String,
}

/// Payload for `search`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    pub matches: Vec<SearchMatch>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_envelope_maps_session_errors() {
        let err: CommandError = SessionError::NeedPath.into();
        assert_eq!(err.code, "E_NEED_PATH");
        assert!(!err.message.is_empty());
    }

    #[test]
    fn view_kind_serializes_as_snake_case() {
        let json = serde_json::to_string(&ViewKind::DepGraph).unwrap();
        assert_eq!(json, "\"dep_graph\"");
    }

    #[test]
    fn apply_result_reports_session_revision() {
        let mut session = Session::new();
        session.mutate(|plan| plan.title = "x".to_owned());
        let result = ApplyResult::from_session(&session, all_views());
        assert_eq!(result.revision, 1);
        assert!(result.dirty);
        assert_eq!(result.scene_stale.len(), 4);
    }

    #[test]
    fn outline_apply_then_scene_reflects_the_text() {
        let state = AppState::new();
        let result = state
            .with_session(|session| {
                session.apply_outline_text("%mcm 1\n%title 演示\n\n- 甲 #t1\n  - 乙 #t2\n");
                Ok(ApplyResult::from_session(session, all_views()))
            })
            .unwrap();
        assert_eq!(result.revision, 1);
        assert!(result.dirty);

        let graph = state
            .with_session(|session| Ok(session.scene(ViewKind::Wbs)))
            .unwrap();
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
    }

    #[test]
    fn edit_apply_reports_stale_views_per_command_kind() {
        // contracts/ipc-commands.md §契约测试 3.
        let state = AppState::new();
        state
            .with_session(|session| {
                session.apply_outline_text(
                    "- 甲 #t1 [2026-09-01..2026-09-02]\n- 乙 #t2 [2026-09-03..2026-09-04]\n! 冻结 #m1 [2026-09-10]\n",
                );
                Ok(())
            })
            .unwrap();

        let result = state
            .with_session(|session| {
                let stale = session
                    .edit(&EditCommand::RenameTask {
                        id: mcm_core::TaskId(1),
                        title: "改名".into(),
                    })
                    .map_err(|e| CommandError::new("E_BAD_TARGET", e.to_string()))?;
                Ok(ApplyResult::from_session(session, stale))
            })
            .unwrap();
        assert_eq!(result.scene_stale.len(), 4);
        assert!(result.undo_depth >= 1);

        // A milestone edit leaves the dependency graph alone.
        let result = state
            .with_session(|session| {
                let stale = session
                    .edit(&EditCommand::RemoveMilestone {
                        id: mcm_core::MilestoneId(1),
                    })
                    .map_err(|e| CommandError::new("E_BAD_TARGET", e.to_string()))?;
                Ok(ApplyResult::from_session(session, stale))
            })
            .unwrap();
        assert!(result.scene_stale.contains(&ViewKind::Milestones));
        assert!(!result.scene_stale.contains(&ViewKind::DepGraph));
    }

    #[test]
    fn undo_and_redo_report_journal_depths() {
        let state = AppState::new();
        state
            .with_session(|session| {
                session.apply_outline_text("- 甲 #t1\n");
                session
                    .edit(&EditCommand::RenameTask {
                        id: mcm_core::TaskId(1),
                        title: "改名".into(),
                    })
                    .map_err(|e| CommandError::new("E_BAD_TARGET", e.to_string()))?;
                Ok(())
            })
            .unwrap();

        let after_undo = state
            .with_session(|session| {
                let stale = session.undo().unwrap_or_default();
                Ok(ApplyResult::from_session(session, stale))
            })
            .unwrap();
        assert!(after_undo.redo_depth >= 1);

        let after_redo = state
            .with_session(|session| {
                let stale = session.redo().unwrap_or_default();
                Ok(ApplyResult::from_session(session, stale))
            })
            .unwrap();
        assert_eq!(after_redo.redo_depth, 0);
    }

    #[test]
    fn undo_on_empty_journal_is_a_no_op_not_an_error() {
        let state = AppState::new();
        let result = state
            .with_session(|session| {
                let stale = session.undo().unwrap_or_default();
                Ok(ApplyResult::from_session(session, stale))
            })
            .unwrap();
        assert!(result.scene_stale.is_empty());
        assert_eq!(result.undo_depth, 0);
    }

    /// Scratch directory that also isolates the prefs location.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let dir = std::env::temp_dir().join(format!("mcm-cmd-{tag}-{stamp}"));
            std::fs::create_dir_all(&dir).expect("scratch");
            Self(dir)
        }

        fn file(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }

        /// A state whose prefs live inside this scratch directory.
        fn state(&self) -> AppState {
            let state = AppState::new();
            if let Ok(mut dir) = state.data_dir.lock() {
                *dir = self.0.clone();
            }
            state
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn save_then_open_round_trips_through_the_commands() {
        let scratch = Scratch::new("saveopen");
        let path = scratch.file("plan.mcm");
        let state = scratch.state();

        state
            .with_session(|session| {
                session.apply_outline_text("%mcm 1\n%title 命令测试\n\n- 甲 #t1\n");
                Ok(())
            })
            .unwrap();

        let saved = state
            .with_session(|session| Ok(session.save(Some(&path))?))
            .expect("save");
        assert!(saved.exists());

        let reopened = AppState::new();
        let text = reopened
            .with_session(|session| {
                session.open(&path)?;
                Ok(session.outline_text())
            })
            .expect("open");
        assert!(text.contains("命令测试"));
    }

    #[test]
    fn saving_without_a_path_surfaces_need_path() {
        let state = AppState::new();
        let error = state
            .with_session(|session| Ok(session.save(None)?))
            .expect_err("must require a path");
        assert_eq!(error.code, "E_NEED_PATH");
    }

    #[test]
    fn opening_a_future_version_surfaces_its_code() {
        let scratch = Scratch::new("future");
        let path = scratch.file("future.mcm");
        std::fs::write(&path, "%mcm 42\n- 甲 #t1\n").expect("write");

        let state = scratch.state();
        let error = state
            .with_session(|session| {
                session.open(&path)?;
                Ok(())
            })
            .expect_err("must refuse");
        assert_eq!(error.code, "E_VERSION_TOO_NEW");
    }

    #[test]
    fn external_modification_is_reported() {
        let scratch = Scratch::new("external");
        let path = scratch.file("plan.mcm");
        let state = scratch.state();

        state
            .with_session(|session| {
                session.apply_outline_text("- 甲 #t1\n");
                session.save(Some(&path))?;
                Ok(())
            })
            .unwrap();
        state.watch.remember(&path);
        assert_eq!(state.watch.check(Some(&path)), ExternalChange::None);

        std::fs::remove_file(&path).expect("remove");
        assert_eq!(state.watch.check(Some(&path)), ExternalChange::Missing);
    }

    #[test]
    fn preferences_persist_and_track_recent_files() {
        let scratch = Scratch::new("prefs");
        let dir = scratch.0.clone();

        let mut prefs = Prefs {
            theme: Some("dark".into()),
            ..Prefs::default()
        };
        prefs.touch_recent("/tmp/one.mcm");
        prefs::save(&dir, &prefs).expect("save prefs");

        let loaded = prefs::load(&dir);
        assert_eq!(loaded.theme.as_deref(), Some("dark"));
        assert_eq!(loaded.recent_files, vec!["/tmp/one.mcm"]);
    }

    #[test]
    fn close_check_reports_unsaved_changes() {
        let state = AppState::new();
        let clean = state
            .with_session(|session| Ok(serde_json::json!({ "dirty": session.is_dirty() })))
            .unwrap();
        assert_eq!(clean["dirty"], serde_json::json!(false));

        state
            .with_session(|session| {
                session.apply_outline_text("- 甲 #t1\n");
                Ok(())
            })
            .unwrap();
        let dirty = state
            .with_session(|session| Ok(serde_json::json!({ "dirty": session.is_dirty() })))
            .unwrap();
        assert_eq!(dirty["dirty"], serde_json::json!(true));
    }

    #[test]
    fn outline_text_get_returns_canonical_text() {
        let state = AppState::new();
        let text = state
            .with_session(|session| {
                session.apply_outline_text("- 无 ID 任务\n");
                Ok(session.outline_text())
            })
            .unwrap();
        assert!(text.starts_with("%mcm 1\n"), "{text}");
        assert!(text.contains(" #t1"), "{text}");
    }

    fn no_accurate() -> CommandResult<Arc<OcrEngine>> {
        Err(CommandError::new("E_MODEL_MISSING", "not installed"))
    }

    #[test]
    fn accurate_import_without_the_model_says_so() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/mcm-import/fixtures/archify-go-service.png");
        let error = import_file(&fixture, OcrModel::Accurate, no_accurate).unwrap_err();
        assert_eq!(error.code, "E_MODEL_MISSING");
    }

    #[test]
    fn ocr_status_reports_a_missing_accurate_model() {
        let dir = std::env::temp_dir().join(format!("mcm-ocr-status-{}", std::process::id()));
        let state = AppState::new();
        *state.data_dir.lock().unwrap() = dir.clone();
        let status = state.ocr_status();
        assert!(!status.accurate_installed);
        assert!(status.accurate_download_bytes > 21_000_000);
        assert!(status.accurate_dir.starts_with(&dir.display().to_string()));
        let error = state.accurate_engine().err().expect("model is missing");
        assert_eq!(error.code, "E_MODEL_MISSING");
    }

    #[test]
    fn diagram_import_reads_an_image_into_outline_text() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/mcm-import/fixtures/archify-go-service.png");
        let imported = import_file(&fixture, OcrModel::Fast, no_accurate).expect("import");
        assert!(
            imported
                .outline
                .starts_with("%mcm 1\n%title archify-go-service\n")
        );
        assert_eq!(imported.report.nodes, 12);
        assert!(imported.outline.contains("internal/server"));
    }

    #[test]
    fn diagram_import_reads_archify_html_by_extension() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/mcm-import/fixtures/archify-go-service.html");
        // HTML never needs the OCR model, even when "accurate" is selected.
        let imported = import_file(&fixture, OcrModel::Accurate, no_accurate).expect("import");
        assert!(imported.outline.starts_with("%mcm 1\n%title Go 服务分层\n"));
        assert_eq!(imported.report.dependencies, 11);
    }

    #[test]
    fn diagram_import_rejects_html_without_archify_markup() {
        let dir = std::env::temp_dir().join(format!("mcm-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("plain.html");
        std::fs::write(&file, "<html><body><p>hi</p></body></html>").unwrap();
        let error = import_file(&file, OcrModel::Fast, no_accurate).unwrap_err();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(error.code, "E_IMPORT");
    }

    #[test]
    fn diagram_source_read_serves_only_markdown_and_mermaid() {
        let dir = std::env::temp_dir().join(format!("mcm-source-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let md = dir.join("架构.md");
        std::fs::write(&md, "# 标题\n\n```mermaid\nflowchart LR\n  a --> b\n```\n").unwrap();
        let secret = dir.join("secret.txt");
        std::fs::write(&secret, "token").unwrap();

        let source = read_diagram_source(&md).expect("markdown is readable");
        let mdx = dir.join("guide.MDX");
        std::fs::write(&mdx, "import X from './x'\n\n<X />\n").unwrap();
        assert_eq!(
            read_diagram_source(&mdx).expect("mdx is readable").name,
            "guide"
        );
        assert_eq!(source.name, "架构");
        assert!(source.text.contains("flowchart LR"));
        let refused = read_diagram_source(&secret).unwrap_err();
        assert_eq!(refused.code, "E_IMPORT");
        let missing = read_diagram_source(&dir.join("gone.mmd")).unwrap_err();
        assert_eq!(missing.code, "E_FILE_IO");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn graph_import_turns_a_parsed_graph_into_outline_text() {
        let spec: GraphSpec = serde_json::from_value(serde_json::json!({
            "title": "",
            "nodes": [{"id": "a", "label": "网关"}, {"id": "b", "label": "订单"}],
            "groups": [{"id": "g", "label": "核心", "members": ["b"]}],
            "edges": [{"from": "a", "to": "g"}]
        }))
        .unwrap();
        let imported = graph_import(spec, "架构".into()).expect("import");
        assert!(
            imported.outline.starts_with("%mcm 1\n%title 架构\n"),
            "{}",
            imported.outline
        );
        assert_eq!(imported.report.dependencies, 1);
        assert_eq!(imported.report.groups, 1);
    }

    #[test]
    fn diagram_import_surfaces_unreadable_files() {
        let missing = import_file(
            Path::new("/definitely/not/here.png"),
            OcrModel::Fast,
            no_accurate,
        )
        .unwrap_err();
        assert_eq!(missing.code, "E_FILE_IO");
    }

    #[test]
    fn app_state_exposes_fresh_session() {
        let state = AppState::new();
        let revision = state
            .with_session(|session| Ok(session.revision()))
            .unwrap();
        assert_eq!(revision, 0);
    }
}

//! XMind exporter: a ZIP of three JSON payloads that XMind 2020–2026 opens as
//! a fully editable mind map (contracts/export-xmind.md).

pub mod model;
pub mod verify;
pub mod writer;

use std::io::Write as _;
use std::path::{Path, PathBuf};

use mcm_core::model::Plan;
use zip::write::SimpleFileOptions;

use crate::report::{ExportError, ExportReport};

pub use writer::{XmindPayload, build};

/// Entry names, in the order the official generator writes them.
const ENTRIES: [&str; 3] = ["content.json", "metadata.json", "manifest.json"];

/// Exports `plan` to `path`, returning the report.
///
/// The payload is self-checked before the file is created, and written through
/// a temp file so a failure never leaves a half-written `.xmind` behind.
pub fn export(plan: &Plan, path: &Path) -> Result<ExportReport, ExportError> {
    let payload = build(plan, &path.display().to_string());
    verify::check_payload(&payload.content, &payload.metadata, &payload.manifest)?;

    let bytes = zip_payload(&payload)?;
    // Re-open the archive we just built and re-run the checks on its contents.
    verify_archive(&bytes)?;
    write_atomically(path, &bytes)?;
    Ok(payload.report)
}

/// Builds the ZIP in memory. Entries are STORED (uncompressed), which is what
/// both official XMind generators emit (research-xmind.md §5).
fn zip_payload(payload: &XmindPayload) -> Result<Vec<u8>, ExportError> {
    let mut buffer = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buffer);
        let mut zip = zip::ZipWriter::new(cursor);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        for name in ENTRIES {
            let contents = match name {
                "content.json" => &payload.content,
                "metadata.json" => &payload.metadata,
                _ => &payload.manifest,
            };
            zip.start_file(name, options)
                .map_err(|error| ExportError::Io(format!("无法写入 {name}：{error}")))?;
            zip.write_all(contents.as_bytes())
                .map_err(|error| ExportError::Io(format!("无法写入 {name}：{error}")))?;
        }
        zip.finish()
            .map_err(|error| ExportError::Io(format!("无法完成打包：{error}")))?;
    }
    Ok(buffer)
}

/// Unpacks the freshly built archive and re-validates it.
fn verify_archive(bytes: &[u8]) -> Result<(), ExportError> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|error| ExportError::SelfCheck(format!("生成的包无法解开：{error}")))?;

    let mut names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    names.sort();
    let mut expected: Vec<String> = ENTRIES.iter().map(|name| (*name).to_owned()).collect();
    expected.sort();
    if names != expected {
        return Err(ExportError::SelfCheck(format!("包内条目不符：{names:?}")));
    }

    let mut read = |name: &str| -> Result<String, ExportError> {
        use std::io::Read as _;
        let mut file = archive
            .by_name(name)
            .map_err(|error| ExportError::SelfCheck(format!("缺少 {name}：{error}")))?;
        if file.compression() != zip::CompressionMethod::Stored {
            return Err(ExportError::SelfCheck(format!("{name} 未使用 STORE 存储")));
        }
        let mut text = String::new();
        file.read_to_string(&mut text)
            .map_err(|error| ExportError::SelfCheck(format!("{name} 不是 UTF-8：{error}")))?;
        Ok(text)
    };

    let content = read("content.json")?;
    let metadata = read("metadata.json")?;
    let manifest = read("manifest.json")?;
    verify::check_payload(&content, &metadata, &manifest)
}

/// Temp file + rename, so an interrupted export cannot corrupt an existing file.
fn write_atomically(target: &Path, bytes: &[u8]) -> Result<(), ExportError> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|error| ExportError::Io(format!("无法创建目录 {}：{error}", parent.display())))?;

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("export.xmind");
    let temp: PathBuf = parent.join(format!(".{file_name}.tmp-{stamp}"));

    let write = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(error) = write {
        let _ = std::fs::remove_file(&temp);
        return Err(ExportError::Io(format!("写入临时文件失败：{error}")));
    }
    if let Err(error) = std::fs::rename(&temp, target) {
        let _ = std::fs::remove_file(&temp);
        return Err(ExportError::Io(format!(
            "无法写入 {}：{error}",
            target.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcm_core::outline::parse;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let dir = std::env::temp_dir().join(format!("mcm-xmind-{tag}-{stamp}"));
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

    const SAMPLE: &str =
        "%mcm 1\n%title 打包测试\n\n- 甲 #t1\n  - 乙 #t2\n! 冻结 #m1 [2026-09-30] <-t2\n";

    #[test]
    fn export_writes_a_readable_archive() {
        let scratch = Scratch::new("write");
        let path = scratch.file("plan.xmind");
        let plan = parse(SAMPLE).plan;

        let report = export(&plan, &path).expect("export");
        assert!(path.exists());
        assert_eq!(report.format, crate::report::ExportFormat::Xmind);

        let file = std::fs::File::open(&path).expect("open");
        let archive = zip::ZipArchive::new(file).expect("zip");
        let mut names: Vec<String> = archive.file_names().map(str::to_owned).collect();
        names.sort();
        assert_eq!(
            names,
            vec!["content.json", "manifest.json", "metadata.json"]
        );
        assert_eq!(archive.len(), 3, "no extra entries");
    }

    #[test]
    fn entries_are_stored_uncompressed() {
        let scratch = Scratch::new("stored");
        let path = scratch.file("plan.xmind");
        export(&parse(SAMPLE).plan, &path).expect("export");

        let file = std::fs::File::open(&path).expect("open");
        let mut archive = zip::ZipArchive::new(file).expect("zip");
        for index in 0..archive.len() {
            let entry = archive.by_index(index).expect("entry");
            assert_eq!(
                entry.compression(),
                zip::CompressionMethod::Stored,
                "{} must be stored",
                entry.name()
            );
        }
    }

    #[test]
    fn export_is_byte_stable() {
        let scratch = Scratch::new("stable");
        let plan = parse(SAMPLE).plan;
        let first = scratch.file("a.xmind");
        let second = scratch.file("b.xmind");
        export(&plan, &first).expect("first");
        export(&plan, &second).expect("second");

        // Compare payloads rather than raw bytes: zip stores timestamps.
        let read_content = |path: &Path| {
            use std::io::Read as _;
            let file = std::fs::File::open(path).expect("open");
            let mut archive = zip::ZipArchive::new(file).expect("zip");
            let mut text = String::new();
            archive
                .by_name("content.json")
                .expect("content")
                .read_to_string(&mut text)
                .expect("read");
            text
        };
        assert_eq!(read_content(&first), read_content(&second));
    }

    #[test]
    fn a_failed_export_leaves_no_partial_file() {
        let scratch = Scratch::new("partial");
        // A directory cannot be replaced by the rename.
        let blocked = scratch.file("blocked.xmind");
        std::fs::create_dir_all(&blocked).expect("dir");
        let error = export(&parse(SAMPLE).plan, &blocked).expect_err("must fail");
        assert_eq!(error.code(), "E_EXPORT_IO");

        let leftovers: Vec<_> = std::fs::read_dir(&scratch.0)
            .expect("read dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "temp files must be cleaned up");
    }

    #[test]
    fn empty_plans_export_successfully() {
        let scratch = Scratch::new("empty");
        let path = scratch.file("empty.xmind");
        export(&Plan::empty(), &path).expect("export");
        assert!(path.exists());
    }
}

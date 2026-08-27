//! Visio exporter: an OPC package Visio 2016+ opens without a repair prompt,
//! with editable shapes and connectors that stay glued
//! (contracts/export-vsdx.md).

pub mod document;
pub mod masters;
pub mod opc;
pub mod page;
pub mod verify;

use std::io::Write as _;
use std::path::{Path, PathBuf};

use mcm_core::model::Plan;

use crate::report::{ExportError, ExportFormat, ExportReport};

use opc::{
    Part, REL_APP, REL_CORE, REL_DOCUMENT, REL_MASTER, REL_MASTERS, REL_PAGE, REL_PAGES,
    REL_WINDOWS, Rel, content_types, relationships, zip_package,
};

/// Content types Visio requires, verbatim from a real saved file.
const CT_DOCUMENT: &str = "application/vnd.ms-visio.drawing.main+xml";
const CT_MASTERS: &str = "application/vnd.ms-visio.masters+xml";
const CT_MASTER: &str = "application/vnd.ms-visio.master+xml";
const CT_PAGES: &str = "application/vnd.ms-visio.pages+xml";
const CT_PAGE: &str = "application/vnd.ms-visio.page+xml";
const CT_WINDOWS: &str = "application/vnd.ms-visio.windows+xml";
const CT_CORE: &str = "application/vnd.openxmlformats-package.core-properties+xml";
const CT_APP: &str = "application/vnd.openxmlformats-officedocument.extended-properties+xml";

/// Builds every part of the package for `plan`.
#[must_use]
pub fn build_parts(plan: &Plan, output_path: &str) -> (Vec<Part>, ExportReport) {
    let mut report = ExportReport::new(ExportFormat::Vsdx, output_path);
    let (page_xml, geometry) = page::build_page(plan, &mut report);

    let mut parts = vec![
        Part::typed("visio/document.xml", CT_DOCUMENT, document::document_xml()),
        Part::typed(
            "visio/pages/pages.xml",
            CT_PAGES,
            document::pages_xml(geometry.width_in, geometry.height_in),
        ),
        Part::typed("visio/pages/page1.xml", CT_PAGE, page_xml),
        Part::typed(
            "visio/masters/masters.xml",
            CT_MASTERS,
            masters::masters_xml(),
        ),
        Part::typed(
            "visio/masters/master1.xml",
            CT_MASTER,
            masters::master1_xml(),
        ),
        Part::typed("visio/windows.xml", CT_WINDOWS, document::windows_xml()),
        Part::typed(
            "docProps/core.xml",
            CT_CORE,
            document::core_xml(&plan.title),
        ),
        Part::typed("docProps/app.xml", CT_APP, document::app_xml()),
    ];

    // Relationship chain: package → document → {pages, masters, windows}.
    parts.push(Part::rels(
        "_rels/.rels",
        relationships(&[
            Rel::new("rId1", REL_DOCUMENT, "visio/document.xml"),
            Rel::new("rId2", REL_CORE, "docProps/core.xml"),
            Rel::new("rId3", REL_APP, "docProps/app.xml"),
        ]),
    ));
    parts.push(Part::rels(
        "visio/_rels/document.xml.rels",
        relationships(&[
            Rel::new("rId1", REL_PAGES, "pages/pages.xml"),
            Rel::new("rId2", REL_MASTERS, "masters/masters.xml"),
            Rel::new("rId3", REL_WINDOWS, "windows.xml"),
        ]),
    ));
    parts.push(Part::rels(
        "visio/pages/_rels/pages.xml.rels",
        relationships(&[Rel::new("rId1", REL_PAGE, "page1.xml")]),
    ));
    parts.push(Part::rels(
        "visio/masters/_rels/masters.xml.rels",
        relationships(&[Rel::new("rId1", REL_MASTER, "master1.xml")]),
    ));

    // Content types must list every typed part, so build it last.
    let types = content_types(&parts);
    parts.push(Part {
        name: "[Content_Types].xml".to_owned(),
        content_type: None,
        data: types,
    });

    (parts, report)
}

/// Exports `plan` to `path`, self-checking before anything is written.
pub fn export(plan: &Plan, path: &Path) -> Result<ExportReport, ExportError> {
    let (parts, report) = build_parts(plan, &path.display().to_string());
    verify::check_parts(&parts)?;

    let bytes = zip_package(&parts)?;
    verify::check_archive(&bytes)?;
    write_atomically(path, &bytes)?;
    Ok(report)
}

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
        .unwrap_or("export.vsdx");
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
            let dir = std::env::temp_dir().join(format!("mcm-vsdx-{tag}-{stamp}"));
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
        "%mcm 1\n%title Visio 包测试\n\n- 甲 #t1\n- 乙 #t2 <-t1\n! 冻结 #m1 [2026-09-30] <-t2\n";

    #[test]
    fn package_contains_every_required_part() {
        let (parts, _) = build_parts(&parse(SAMPLE).plan, "/tmp/a.vsdx");
        let names: Vec<&str> = parts.iter().map(|part| part.name.as_str()).collect();
        for required in [
            "[Content_Types].xml",
            "_rels/.rels",
            "visio/document.xml",
            "visio/_rels/document.xml.rels",
            "visio/pages/pages.xml",
            "visio/pages/_rels/pages.xml.rels",
            "visio/pages/page1.xml",
            "visio/masters/masters.xml",
            "visio/masters/master1.xml",
        ] {
            assert!(names.contains(&required), "missing {required}");
        }
    }

    #[test]
    fn content_types_cover_every_typed_part() {
        let (parts, _) = build_parts(&parse(SAMPLE).plan, "/tmp/a.vsdx");
        let types = parts
            .iter()
            .find(|part| part.name == "[Content_Types].xml")
            .expect("content types");
        for part in &parts {
            if part.content_type.is_some() {
                assert!(
                    types.data.contains(&format!("/{}", part.name)),
                    "no override for {}",
                    part.name
                );
            }
        }
    }

    #[test]
    fn export_writes_a_readable_package() {
        let scratch = Scratch::new("write");
        let path = scratch.file("plan.vsdx");
        let report = export(&parse(SAMPLE).plan, &path).expect("export");
        assert!(path.exists());
        assert_eq!(report.format, ExportFormat::Vsdx);

        let file = std::fs::File::open(&path).expect("open");
        let archive = zip::ZipArchive::new(file).expect("valid zip");
        assert!(archive.len() >= 9);
    }

    #[test]
    fn a_failed_export_leaves_no_partial_file() {
        let scratch = Scratch::new("partial");
        let blocked = scratch.file("blocked.vsdx");
        std::fs::create_dir_all(&blocked).expect("dir");
        let error = export(&parse(SAMPLE).plan, &blocked).expect_err("must fail");
        assert_eq!(error.code(), "E_EXPORT_IO");

        let temps: Vec<_> = std::fs::read_dir(&scratch.0)
            .expect("read dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(temps.is_empty());
    }

    #[test]
    fn empty_plans_export_successfully() {
        let scratch = Scratch::new("empty");
        let path = scratch.file("empty.vsdx");
        export(&Plan::empty(), &path).expect("export");
        assert!(path.exists());
    }
}

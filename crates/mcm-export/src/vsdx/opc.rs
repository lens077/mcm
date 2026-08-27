//! OPC package assembly for `.vsdx` (contracts/export-vsdx.md §OPC 包结构).
//!
//! Visio refuses a package whose content types or relationship chain are wrong,
//! so both are built from one declarative part list rather than by hand.

use std::io::Write as _;

use zip::write::SimpleFileOptions;

use crate::report::ExportError;

/// Visio's desktop namespace. MS-VSDX documents the SharePoint web-drawing
/// namespace instead; real files use this one (research-vsdx.md §2).
pub const NS_MAIN: &str = "http://schemas.microsoft.com/office/visio/2012/main";
pub const NS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// Relationship type strings, exactly as Visio writes them.
pub const REL_DOCUMENT: &str = "http://schemas.microsoft.com/visio/2010/relationships/document";
pub const REL_PAGES: &str = "http://schemas.microsoft.com/visio/2010/relationships/pages";
pub const REL_PAGE: &str = "http://schemas.microsoft.com/visio/2010/relationships/page";
pub const REL_MASTERS: &str = "http://schemas.microsoft.com/visio/2010/relationships/masters";
pub const REL_MASTER: &str = "http://schemas.microsoft.com/visio/2010/relationships/master";
pub const REL_WINDOWS: &str = "http://schemas.microsoft.com/visio/2010/relationships/windows";
pub const REL_CORE: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
pub const REL_APP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties";

/// One part in the package: archive-root-relative name plus UTF-8 contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub name: String,
    pub content_type: Option<String>,
    pub data: String,
}

impl Part {
    /// A part that needs a `<Override>` content-type entry.
    #[must_use]
    pub fn typed(name: &str, content_type: &str, data: String) -> Self {
        Self {
            name: name.to_owned(),
            content_type: Some(content_type.to_owned()),
            data,
        }
    }

    /// A `.rels` part, covered by the `Default` extension entry.
    #[must_use]
    pub fn rels(name: &str, data: String) -> Self {
        Self {
            name: name.to_owned(),
            content_type: None,
            data,
        }
    }
}

/// XML declaration Visio writes; no BOM, UTF-8.
pub const XML_DECL: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n";

/// Escapes text for XML content and attribute values.
#[must_use]
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Builds `[Content_Types].xml` from the typed parts.
#[must_use]
pub fn content_types(parts: &[Part]) -> String {
    let mut xml = String::from(XML_DECL);
    xml.push_str("<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">");
    xml.push_str(
        "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
    );
    xml.push_str("<Default Extension=\"xml\" ContentType=\"application/xml\"/>");
    for part in parts {
        if let Some(content_type) = &part.content_type {
            xml.push_str(&format!(
                "<Override PartName=\"/{}\" ContentType=\"{}\"/>",
                escape(&part.name),
                escape(content_type)
            ));
        }
    }
    xml.push_str("</Types>");
    xml
}

/// One relationship inside a `.rels` part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rel {
    pub id: String,
    pub rel_type: String,
    pub target: String,
}

impl Rel {
    #[must_use]
    pub fn new(id: &str, rel_type: &str, target: &str) -> Self {
        Self {
            id: id.to_owned(),
            rel_type: rel_type.to_owned(),
            target: target.to_owned(),
        }
    }
}

/// Serializes a `.rels` part.
#[must_use]
pub fn relationships(rels: &[Rel]) -> String {
    let mut xml = String::from(XML_DECL);
    xml.push_str(
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    );
    for rel in rels {
        xml.push_str(&format!(
            "<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"/>",
            escape(&rel.id),
            escape(&rel.rel_type),
            escape(&rel.target)
        ));
    }
    xml.push_str("</Relationships>");
    xml
}

/// Zips the package. `[Content_Types].xml` goes first, as Visio writes it, and
/// no directory entries are emitted (research-vsdx.md §6).
pub fn zip_package(parts: &[Part]) -> Result<Vec<u8>, ExportError> {
    let mut buffer = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buffer);
        let mut zip = zip::ZipWriter::new(cursor);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        // Content types must be the first entry.
        let mut ordered: Vec<&Part> = Vec::with_capacity(parts.len());
        if let Some(types) = parts.iter().find(|part| part.name == "[Content_Types].xml") {
            ordered.push(types);
        }
        for part in parts {
            if part.name != "[Content_Types].xml" {
                ordered.push(part);
            }
        }

        for part in ordered {
            zip.start_file(&part.name, options)
                .map_err(|error| ExportError::Io(format!("无法写入 {}：{error}", part.name)))?;
            zip.write_all(part.data.as_bytes())
                .map_err(|error| ExportError::Io(format!("无法写入 {}：{error}", part.name)))?;
        }
        zip.finish()
            .map_err(|error| ExportError::Io(format!("无法完成打包：{error}")))?;
    }
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_parts() -> Vec<Part> {
        vec![
            Part::typed(
                "visio/document.xml",
                "application/vnd.ms-visio.drawing.main+xml",
                "<VisioDocument/>".into(),
            ),
            Part::rels(
                "_rels/.rels",
                relationships(&[Rel::new("rId1", REL_DOCUMENT, "visio/document.xml")]),
            ),
        ]
    }

    #[test]
    fn content_types_declare_both_defaults() {
        let xml = content_types(&sample_parts());
        assert!(xml.contains("Extension=\"rels\""), "{xml}");
        assert!(xml.contains("Extension=\"xml\""), "{xml}");
    }

    #[test]
    fn content_types_override_every_typed_part() {
        let xml = content_types(&sample_parts());
        assert!(xml.contains("PartName=\"/visio/document.xml\""), "{xml}");
        assert!(
            xml.contains("application/vnd.ms-visio.drawing.main+xml"),
            "{xml}"
        );
    }

    #[test]
    fn rels_parts_get_no_override() {
        let xml = content_types(&sample_parts());
        assert!(
            !xml.contains("_rels/.rels"),
            "rels are covered by the Default entry"
        );
    }

    #[test]
    fn relationships_serialize_id_type_and_target() {
        let xml = relationships(&[Rel::new("rId1", REL_PAGES, "pages/pages.xml")]);
        assert!(xml.contains("Id=\"rId1\""), "{xml}");
        assert!(xml.contains(REL_PAGES), "{xml}");
        assert!(xml.contains("Target=\"pages/pages.xml\""), "{xml}");
    }

    #[test]
    fn xml_declaration_has_no_bom() {
        assert!(XML_DECL.starts_with("<?xml"), "no BOM allowed");
        assert!(XML_DECL.contains("utf-8"));
    }

    #[test]
    fn escaping_covers_every_xml_metacharacter() {
        assert_eq!(escape("a<b>c&d\"e'f"), "a&lt;b&gt;c&amp;d&quot;e&apos;f");
        assert_eq!(escape("中文 🚀"), "中文 🚀", "unicode passes through");
    }

    #[test]
    fn content_types_is_the_first_zip_entry() {
        let mut parts = sample_parts();
        parts.push(Part::typed(
            "[Content_Types].xml",
            "",
            content_types(&sample_parts()),
        ));
        let bytes = zip_package(&parts).expect("zip");
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("valid zip");
        let first = archive.by_index(0).expect("first entry");
        assert_eq!(first.name(), "[Content_Types].xml");
    }

    #[test]
    fn package_has_no_directory_entries() {
        let bytes = zip_package(&sample_parts()).expect("zip");
        let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("valid zip");
        for name in archive.file_names() {
            assert!(!name.ends_with('/'), "directory entry: {name}");
        }
    }

    #[test]
    fn part_names_are_archive_root_relative() {
        let bytes = zip_package(&sample_parts()).expect("zip");
        let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("valid zip");
        for name in archive.file_names() {
            assert!(!name.starts_with('/'), "leading slash: {name}");
            assert!(!name.contains(".."), "traversal: {name}");
        }
    }
}

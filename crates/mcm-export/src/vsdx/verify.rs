//! Pre-write self check (contracts/export-vsdx.md §生成规则 7).
//!
//! Every failure mode listed in research-vsdx.md §6 is checked here, because
//! Visio's own reaction is either a repair prompt or — worse — silently
//! dropping shapes.

use std::collections::{BTreeMap, BTreeSet};

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::report::ExportError;

use super::opc::Part;

fn fail(message: impl Into<String>) -> ExportError {
    ExportError::SelfCheck(message.into())
}

/// Validates the assembled parts before they are zipped.
pub fn check_parts(parts: &[Part]) -> Result<(), ExportError> {
    let by_name: BTreeMap<&str, &Part> = parts
        .iter()
        .map(|part| (part.name.as_str(), part))
        .collect();

    for required in [
        "[Content_Types].xml",
        "_rels/.rels",
        "visio/document.xml",
        "visio/_rels/document.xml.rels",
        "visio/pages/pages.xml",
        "visio/pages/_rels/pages.xml.rels",
        "visio/pages/page1.xml",
    ] {
        if !by_name.contains_key(required) {
            return Err(fail(format!("缺少必需 part：{required}")));
        }
    }

    // Every part must be well-formed XML without a BOM.
    for part in parts {
        if part.data.starts_with('\u{feff}') {
            return Err(fail(format!("{} 含 BOM", part.name)));
        }
        well_formed(&part.data)
            .map_err(|error| fail(format!("{} 不是良构 XML：{error}", part.name)))?;
    }

    // Content types must override every typed part.
    let types = by_name["[Content_Types].xml"];
    for part in parts {
        if part.content_type.is_some() && !types.data.contains(&format!("/{}", part.name)) {
            return Err(fail(format!(
                "[Content_Types].xml 缺少 {} 的 Override",
                part.name
            )));
        }
    }

    check_rel_chain(&by_name)?;
    check_page(by_name["visio/pages/page1.xml"])?;
    Ok(())
}

/// Follows package → document → pages → page and document → masters.
fn check_rel_chain(by_name: &BTreeMap<&str, &Part>) -> Result<(), ExportError> {
    let package = by_name["_rels/.rels"];
    if !package.data.contains("visio/document.xml") {
        return Err(fail("_rels/.rels 未指向 visio/document.xml"));
    }

    let document_rels = by_name["visio/_rels/document.xml.rels"];
    if !document_rels.data.contains("pages/pages.xml") {
        return Err(fail("document.xml.rels 未指向 pages.xml"));
    }
    if by_name.contains_key("visio/masters/masters.xml")
        && !document_rels.data.contains("masters/masters.xml")
    {
        return Err(fail("存在 masters part 但 document.xml.rels 未引用"));
    }

    let pages_rels = by_name["visio/pages/_rels/pages.xml.rels"];
    if !pages_rels.data.contains("page1.xml") {
        return Err(fail("pages.xml.rels 未指向 page1.xml"));
    }
    // The page's <Rel r:id> must match an id declared in pages.xml.rels.
    let pages = by_name["visio/pages/pages.xml"];
    let rel_id = extract_attribute(&pages.data, "Rel", "r:id")
        .ok_or_else(|| fail("pages.xml 缺少 <Rel r:id>"))?;
    if !pages_rels.data.contains(&format!("Id=\"{rel_id}\"")) {
        return Err(fail(format!(
            "pages.xml 的 Rel r:id={rel_id} 在 rels 中不存在"
        )));
    }

    if by_name.contains_key("visio/masters/master1.xml") {
        let masters_rels = by_name
            .get("visio/masters/_rels/masters.xml.rels")
            .ok_or_else(|| fail("缺少 masters.xml.rels"))?;
        if !masters_rels.data.contains("master1.xml") {
            return Err(fail("masters.xml.rels 未指向 master1.xml"));
        }
    }
    Ok(())
}

/// Shape ids must be unique, and every Connect must reference a real shape.
fn check_page(page: &Part) -> Result<(), ExportError> {
    let mut shape_ids: BTreeSet<String> = BTreeSet::new();
    let mut reader = Reader::from_str(&page.data);
    let mut buffer = Vec::new();
    let mut connects: Vec<(String, String)> = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                let name = element.name();
                let tag = String::from_utf8_lossy(name.as_ref()).to_string();
                if tag == "Shape" {
                    if let Some(id) = attribute_of(&element, "ID") {
                        if !shape_ids.insert(id.clone()) {
                            // Visio silently drops duplicates, losing content.
                            return Err(fail(format!("Shape ID 重复：{id}")));
                        }
                    } else {
                        return Err(fail("Shape 缺少 ID"));
                    }
                } else if tag == "Connect" {
                    let from = attribute_of(&element, "FromSheet")
                        .ok_or_else(|| fail("Connect 缺少 FromSheet"))?;
                    let to = attribute_of(&element, "ToSheet")
                        .ok_or_else(|| fail("Connect 缺少 ToSheet"))?;
                    connects.push((from, to));
                }
            }
            Ok(_) => {}
            Err(error) => return Err(fail(format!("page1.xml 解析失败：{error}"))),
        }
        buffer.clear();
    }

    for (from, to) in &connects {
        if !shape_ids.contains(from) {
            return Err(fail(format!("Connect.FromSheet 指向不存在的形状：{from}")));
        }
        if !shape_ids.contains(to) {
            return Err(fail(format!("Connect.ToSheet 指向不存在的形状：{to}")));
        }
    }
    Ok(())
}

fn attribute_of(element: &quick_xml::events::BytesStart<'_>, name: &str) -> Option<String> {
    element.attributes().flatten().find_map(|attr| {
        if attr.key.as_ref() == name.as_bytes() {
            Some(String::from_utf8_lossy(&attr.value).to_string())
        } else {
            None
        }
    })
}

/// Crude but dependency-free attribute lookup for a named element.
fn extract_attribute(xml: &str, element: &str, attribute: &str) -> Option<String> {
    let needle = format!("<{element} ");
    let start = xml.find(&needle)? + needle.len();
    let rest = &xml[start..];
    let attr_needle = format!("{attribute}=\"");
    let attr_start = rest.find(&attr_needle)? + attr_needle.len();
    let value = &rest[attr_start..];
    let end = value.find('"')?;
    Some(value[..end].to_owned())
}

fn well_formed(xml: &str) -> Result<(), String> {
    let mut reader = Reader::from_str(xml);
    // Unbalanced tags must be an error, not a silently tolerated quirk.
    reader.config_mut().check_end_names = true;
    let mut buffer = Vec::new();
    let mut depth = 0i32;
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(_)) => depth += 1,
            Ok(Event::End(_)) => depth -= 1,
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
        buffer.clear();
    }
    if depth != 0 {
        return Err(format!("标签未闭合（深度 {depth}）"));
    }
    Ok(())
}

/// Re-opens the zipped package and re-runs the part checks.
pub fn check_archive(bytes: &[u8]) -> Result<(), ExportError> {
    use std::io::Read as _;

    let cursor = std::io::Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|error| fail(format!("生成的包无法解开：{error}")))?;

    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    for name in &names {
        if name.ends_with('/') {
            return Err(fail(format!("包内存在目录条目：{name}")));
        }
    }

    let mut parts = Vec::new();
    for name in names {
        let mut data = String::new();
        archive
            .by_name(&name)
            .map_err(|error| fail(format!("无法读取 {name}：{error}")))?
            .read_to_string(&mut data)
            .map_err(|error| fail(format!("{name} 不是 UTF-8：{error}")))?;
        // Content type is irrelevant on read-back; the override check already ran.
        parts.push(Part {
            name,
            content_type: None,
            data,
        });
    }

    // Re-check structure, skipping the override cross-check (types are inside).
    let by_name: BTreeMap<&str, &Part> = parts
        .iter()
        .map(|part| (part.name.as_str(), part))
        .collect();
    for required in [
        "[Content_Types].xml",
        "_rels/.rels",
        "visio/pages/page1.xml",
    ] {
        if !by_name.contains_key(required) {
            return Err(fail(format!("解包后缺少 {required}")));
        }
    }
    check_rel_chain(&by_name)?;
    check_page(by_name["visio/pages/page1.xml"])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vsdx::build_parts;
    use mcm_core::outline::parse;

    const SAMPLE: &str = "%mcm 1\n- 甲 #t1\n- 乙 #t2 <-t1\n! 冻结 #m1 [2026-09-30] <-t2\n";

    fn parts() -> Vec<Part> {
        build_parts(&parse(SAMPLE).plan, "/tmp/a.vsdx").0
    }

    #[test]
    fn a_generated_package_passes() {
        assert!(check_parts(&parts()).is_ok());
    }

    #[test]
    fn missing_required_parts_are_rejected() {
        let mut parts = parts();
        parts.retain(|part| part.name != "visio/pages/page1.xml");
        let error = check_parts(&parts).expect_err("must fail");
        assert!(error.to_string().contains("page1.xml"), "{error}");
    }

    #[test]
    fn a_broken_rel_chain_is_rejected() {
        let mut parts = parts();
        for part in &mut parts {
            if part.name == "visio/_rels/document.xml.rels" {
                part.data = part.data.replace("pages/pages.xml", "pages/missing.xml");
            }
        }
        let error = check_parts(&parts).expect_err("must fail");
        assert!(error.to_string().contains("pages.xml"), "{error}");
    }

    #[test]
    fn a_mismatched_page_rel_id_is_rejected() {
        let mut parts = parts();
        for part in &mut parts {
            if part.name == "visio/pages/pages.xml" {
                part.data = part.data.replace("r:id=\"rId1\"", "r:id=\"rId99\"");
            }
        }
        let error = check_parts(&parts).expect_err("must fail");
        assert!(error.to_string().contains("rId99"), "{error}");
    }

    #[test]
    fn duplicate_shape_ids_are_rejected() {
        let mut parts = parts();
        for part in &mut parts {
            if part.name == "visio/pages/page1.xml" {
                part.data = part.data.replace("<Shape ID=\"2\"", "<Shape ID=\"1\"");
            }
        }
        let error = check_parts(&parts).expect_err("must fail");
        assert!(error.to_string().contains("重复"), "{error}");
    }

    #[test]
    fn dangling_connects_are_rejected() {
        let mut parts = parts();
        for part in &mut parts {
            if part.name == "visio/pages/page1.xml" {
                part.data = part.data.replace("ToSheet=\"1\"", "ToSheet=\"999\"");
            }
        }
        let error = check_parts(&parts).expect_err("must fail");
        assert!(error.to_string().contains("999"), "{error}");
    }

    #[test]
    fn malformed_xml_is_rejected() {
        let mut parts = parts();
        for part in &mut parts {
            if part.name == "visio/document.xml" {
                part.data = "<VisioDocument><unclosed>".to_owned();
            }
        }
        assert!(check_parts(&parts).is_err());
    }

    #[test]
    fn a_bom_is_rejected() {
        let mut parts = parts();
        for part in &mut parts {
            if part.name == "visio/document.xml" {
                part.data = format!("\u{feff}{}", part.data);
            }
        }
        let error = check_parts(&parts).expect_err("must fail");
        assert!(error.to_string().contains("BOM"), "{error}");
    }

    #[test]
    fn missing_content_type_overrides_are_rejected() {
        let mut parts = parts();
        for part in &mut parts {
            if part.name == "[Content_Types].xml" {
                part.data = part.data.replace(
                    "<Override PartName=\"/visio/pages/page1.xml\"",
                    "<Override PartName=\"/visio/pages/other.xml\"",
                );
            }
        }
        let error = check_parts(&parts).expect_err("must fail");
        assert!(error.to_string().contains("page1.xml"), "{error}");
    }

    #[test]
    fn the_zipped_archive_passes_read_back_checks() {
        let bytes = crate::vsdx::opc::zip_package(&parts()).expect("zip");
        assert!(check_archive(&bytes).is_ok());
    }
}

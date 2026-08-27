//! Dynamic connector master (contracts/export-vsdx.md §结构决策).
//!
//! Task rectangles are masterless — that is what Visio itself writes — but the
//! connector instances a master so routing behaves correctly and user-drawn
//! connectors merge with ours (research-vsdx.md §4).

use super::opc::{NS_MAIN, NS_REL, XML_DECL};

/// Visio's built-in Dynamic connector identity. `MatchByName` lets Visio merge
/// our master with its own stencil entry.
pub const CONNECTOR_BASE_ID: &str = "{F7290A45-E3AD-11D2-AE4F-006008C9F5A9}";
pub const CONNECTOR_UNIQUE_ID: &str = "{C4C0C4E7-1B69-4A5D-9C4D-7F1E2A3B4C5D}";
/// Master ID referenced by shape instances (`Master='2'`).
pub const CONNECTOR_MASTER_ID: u32 = 2;

/// `visio/masters/masters.xml`.
#[must_use]
pub fn masters_xml() -> String {
    let mut xml = String::from(XML_DECL);
    xml.push_str(&format!(
        "<Masters xmlns=\"{NS_MAIN}\" xmlns:r=\"{NS_REL}\" xml:space=\"preserve\">"
    ));
    xml.push_str(&format!(
        "<Master ID=\"{CONNECTOR_MASTER_ID}\" NameU=\"Dynamic connector\" Name=\"Dynamic connector\" \
IsCustomName=\"0\" IsCustomNameU=\"0\" UniqueID=\"{CONNECTOR_UNIQUE_ID}\" BaseID=\"{CONNECTOR_BASE_ID}\" \
MasterType=\"0\" Hidden=\"0\" MatchByName=\"1\" IconUpdate=\"0\" PatternFlags=\"0\" Prompt=\"\">"
    ));
    // The master's own PageSheet defaults; Visio expects the element to exist.
    xml.push_str(
        "<PageSheet><Cell N=\"PageWidth\" V=\"1\"/><Cell N=\"PageHeight\" V=\"1\"/>\
<Cell N=\"ShdwOffsetX\" V=\"0\"/><Cell N=\"ShdwOffsetY\" V=\"0\"/></PageSheet>",
    );
    xml.push_str("<Rel r:id=\"rId1\"/>");
    xml.push_str("</Master></Masters>");
    xml
}

/// `visio/masters/master1.xml` — the connector geometry and glue defaults.
#[must_use]
pub fn master1_xml() -> String {
    // Kept as a literal so the shape matches the dissected reference file.
    include_str!("../../fixtures/vsdx/dynamic-connector-master.xml").to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masters_xml_declares_the_builtin_base_id() {
        let xml = masters_xml();
        assert!(xml.contains(CONNECTOR_BASE_ID), "{xml}");
        assert!(xml.contains("MatchByName=\"1\""), "{xml}");
        assert!(xml.contains("NameU=\"Dynamic connector\""), "{xml}");
    }

    #[test]
    fn masters_xml_uses_the_desktop_namespace() {
        let xml = masters_xml();
        assert!(xml.contains(NS_MAIN), "{xml}");
        assert!(
            !xml.contains("visio/2011/1/core"),
            "the SharePoint namespace is wrong"
        );
    }

    #[test]
    fn masters_xml_links_to_its_content_part() {
        assert!(masters_xml().contains("<Rel r:id=\"rId1\"/>"));
    }

    #[test]
    fn master_shape_is_a_one_dimensional_connector() {
        let xml = master1_xml();
        // ObjType 2 = routable connector, GlueType 2 = walking glue.
        assert!(xml.contains("N=\"ObjType\" V=\"2\""), "{xml}");
        assert!(xml.contains("N=\"GlueType\" V=\"2\""), "{xml}");
        assert!(xml.contains("N=\"DynFeedback\" V=\"2\""), "{xml}");
    }

    #[test]
    fn master_shape_carries_begin_and_end_cells() {
        let xml = master1_xml();
        for cell in ["BeginX", "BeginY", "EndX", "EndY"] {
            assert!(xml.contains(&format!("N=\"{cell}\"")), "missing {cell}");
        }
    }

    #[test]
    fn master_geometry_starts_with_a_moveto_row() {
        let xml = master1_xml();
        let move_at = xml.find("T=\"MoveTo\" IX=\"1\"").expect("MoveTo row");
        let line_at = xml.find("T=\"LineTo\" IX=\"2\"").expect("LineTo row");
        assert!(move_at < line_at, "geometry must start with MoveTo");
    }

    #[test]
    fn master_content_is_well_formed_xml() {
        let xml = master1_xml();
        assert!(xml.starts_with("<?xml"), "needs a declaration and no BOM");
        assert!(xml.contains("<MasterContents"));
        assert!(xml.trim_end().ends_with("</MasterContents>"));
    }
}

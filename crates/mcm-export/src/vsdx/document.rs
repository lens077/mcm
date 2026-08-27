//! `visio/document.xml`, `pages.xml` and the docProps stubs
//! (contracts/export-vsdx.md §OPC 包结构).

use super::opc::{NS_MAIN, NS_REL, XML_DECL, escape};

/// Document part: settings, a minimal stylesheet chain and face names.
#[must_use]
pub fn document_xml() -> String {
    let mut xml = String::from(XML_DECL);
    xml.push_str(&format!(
        "<VisioDocument xmlns=\"{NS_MAIN}\" xmlns:r=\"{NS_REL}\" xml:space=\"preserve\">"
    ));
    xml.push_str(
        "<DocumentSettings TopPage=\"0\" DefaultTextStyle=\"0\" DefaultLineStyle=\"0\" DefaultFillStyle=\"0\" DefaultGuideStyle=\"0\">\
<GlueSettings>9</GlueSettings><SnapSettings>295</SnapSettings>\
<SnapExtensions>34</SnapExtensions><DynamicGridEnabled>1</DynamicGridEnabled>\
<ProtectStyles>0</ProtectStyles><ProtectShapes>0</ProtectShapes><ProtectMasters>0</ProtectMasters>\
<ProtectBkgnds>0</ProtectBkgnds></DocumentSettings>",
    );
    // Shapes reference style 0 only, so a single "No Style" sheet suffices.
    xml.push_str(
        "<StyleSheets><StyleSheet ID=\"0\" NameU=\"No Style\" Name=\"No Style\">\
<Cell N=\"LineWeight\" V=\"0.01\"/><Cell N=\"LineColor\" V=\"#000000\"/>\
<Cell N=\"LinePattern\" V=\"1\"/><Cell N=\"FillForegnd\" V=\"#ffffff\"/>\
<Cell N=\"FillPattern\" V=\"1\"/><Cell N=\"CharSize\" V=\"0.1666666666666667\"/>\
</StyleSheet></StyleSheets>",
    );
    xml.push_str("</VisioDocument>");
    xml
}

/// Pages part: one page whose `<Rel>` id matches `pages.xml.rels`.
#[must_use]
pub fn pages_xml(width_in: f64, height_in: f64) -> String {
    let mut xml = String::from(XML_DECL);
    xml.push_str(&format!(
        "<Pages xmlns=\"{NS_MAIN}\" xmlns:r=\"{NS_REL}\" xml:space=\"preserve\">"
    ));
    xml.push_str("<Page ID=\"0\" NameU=\"Page-1\" Name=\"Page-1\" ViewScale=\"-1\" ViewCenterX=\"0\" ViewCenterY=\"0\">");
    xml.push_str("<PageSheet>");
    xml.push_str(&format!("<Cell N=\"PageWidth\" V=\"{width_in:.4}\"/>"));
    xml.push_str(&format!("<Cell N=\"PageHeight\" V=\"{height_in:.4}\"/>"));
    xml.push_str(
        "<Cell N=\"ShdwOffsetX\" V=\"0.1181\"/><Cell N=\"ShdwOffsetY\" V=\"-0.1181\"/>\
<Cell N=\"PageScale\" V=\"1\" U=\"IN_F\"/><Cell N=\"DrawingScale\" V=\"1\" U=\"IN_F\"/>\
<Cell N=\"DrawingSizeType\" V=\"3\"/><Cell N=\"DrawingScaleType\" V=\"0\"/>",
    );
    xml.push_str("</PageSheet>");
    xml.push_str("<Rel r:id=\"rId1\"/>");
    xml.push_str("</Page></Pages>");
    xml
}

/// Optional but tidy: window state so Visio opens at a sane zoom.
#[must_use]
pub fn windows_xml() -> String {
    let mut xml = String::from(XML_DECL);
    xml.push_str(&format!(
        "<Windows xmlns=\"{NS_MAIN}\" xmlns:r=\"{NS_REL}\" xml:space=\"preserve\" ClientWidth=\"1440\" ClientHeight=\"900\">"
    ));
    xml.push_str(
        "<Window ID=\"0\" WindowType=\"Drawing\" WindowState=\"1073741824\" Document=\"visio/document.xml\" \
Page=\"0\" ViewScale=\"-1\" ViewCenterX=\"0\" ViewCenterY=\"0\"/>",
    );
    xml.push_str("</Windows>");
    xml
}

#[must_use]
pub fn core_xml(title: &str) -> String {
    let mut xml = String::from(XML_DECL);
    xml.push_str(
        "<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" \
xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">",
    );
    xml.push_str(&format!("<dc:title>{}</dc:title>", escape(title)));
    xml.push_str("<dc:creator>MCM</dc:creator><cp:lastModifiedBy>MCM</cp:lastModifiedBy>");
    xml.push_str("</cp:coreProperties>");
    xml
}

#[must_use]
pub fn app_xml() -> String {
    let mut xml = String::from(XML_DECL);
    xml.push_str(
        "<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\" \
xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\">",
    );
    xml.push_str("<Application>MCM</Application><Company>MCM</Company>");
    xml.push_str("</Properties>");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_uses_the_desktop_namespace() {
        let xml = document_xml();
        assert!(xml.contains(NS_MAIN), "{xml}");
        assert!(!xml.contains("visio/2011/1/core"));
    }

    #[test]
    fn document_ships_a_style_sheet_zero() {
        let xml = document_xml();
        assert!(xml.contains("<StyleSheet ID=\"0\""), "{xml}");
        assert!(xml.contains("NameU=\"No Style\""), "{xml}");
    }

    #[test]
    fn pages_declare_size_in_inches() {
        let xml = pages_xml(11.0, 8.5);
        assert!(xml.contains("N=\"PageWidth\" V=\"11.0000\""), "{xml}");
        assert!(xml.contains("N=\"PageHeight\" V=\"8.5000\""), "{xml}");
    }

    #[test]
    fn pages_bind_to_their_content_part() {
        assert!(pages_xml(8.5, 11.0).contains("<Rel r:id=\"rId1\"/>"));
    }

    #[test]
    fn page_id_is_zero() {
        assert!(pages_xml(8.5, 11.0).contains("<Page ID=\"0\""));
    }

    #[test]
    fn core_properties_escape_the_title() {
        let xml = core_xml("规划 <测试> & 更多");
        assert!(xml.contains("&lt;测试&gt;"), "{xml}");
        assert!(xml.contains("&amp;"), "{xml}");
    }

    #[test]
    fn every_part_starts_with_the_xml_declaration() {
        for xml in [
            document_xml(),
            pages_xml(8.5, 11.0),
            windows_xml(),
            app_xml(),
        ] {
            assert!(xml.starts_with("<?xml"), "missing declaration: {xml}");
            assert!(!xml.starts_with('\u{feff}'), "no BOM allowed");
        }
    }
}

//! archify HTML of the same Go service diagram as `archify-go-service.png`
//! (source spec: `fixtures/archify-go-service.architecture.json`). The HTML
//! path is exact, so it also serves as ground truth for the image path.

use std::collections::BTreeSet;

use mcm_import::{Imported, import_html, import_image};

const HTML: &[u8] = include_bytes!("../fixtures/archify-go-service.html");
const PNG: &[u8] = include_bytes!("../fixtures/archify-go-service.png");

/// Every dependency as (predecessor title, successor title).
fn arrows(imported: &Imported) -> BTreeSet<(String, String)> {
    let parsed = mcm_core::outline::parse(&imported.outline);
    let plan = parsed.plan;
    // OCR may slip on letter case ("PostgresQL"); compare case-insensitively.
    let title = |id| {
        plan.task(id)
            .map(|t| t.title.to_lowercase())
            .unwrap_or_default()
    };
    plan.dependencies
        .iter()
        .map(|d| (title(d.predecessor), title(d.successor)))
        .collect()
}

#[test]
fn archify_html_imports_exactly() {
    let imported = import_html(HTML, "fallback").expect("import");
    let out = &imported.outline;
    assert!(out.starts_with("%mcm 1\n%title Go 服务分层"), "{out}");
    assert_eq!(imported.report.nodes, 12);
    assert_eq!(imported.report.groups, 1);
    assert_eq!(imported.report.dependencies, 11);
    // Text comes from attributes, so it is exact — entities decoded.
    assert!(out.contains("- Kratos Gateway #"), "{out}");
    assert!(out.contains("> discovery:///<注册名>"), "{out}");
    assert!(
        out.contains("- backend/services/{service}/internal/ — 靠目录与 internal/ 强制边界 #"),
        "{out}"
    );
    assert!(out.contains("  - internal/data/models #"), "{out}");
    // Edge labels cannot live in the plan model; they are kept as comments.
    assert!(
        out.contains("# 连线说明：internal/server → Consul：注册"),
        "{out}"
    );
    assert!(
        out.contains("# 连线说明：backend/api/{svc}/v1 → internal/service：生成代码"),
        "{out}"
    );

    let parsed = mcm_core::outline::parse(out);
    assert!(
        parsed.issues.iter().all(|i| !i.is_error()),
        "{:?}",
        parsed.issues
    );
    assert!(
        mcm_core::validate::validate(&parsed.plan)
            .iter()
            .all(|i| !i.is_error())
    );
}

#[test]
fn image_and_html_of_the_same_diagram_agree_on_every_arrow() {
    let from_html = arrows(&import_html(HTML, "x").unwrap());
    let from_image = arrows(&import_image(PNG, "x").unwrap());
    assert_eq!(from_html.len(), 11);
    assert_eq!(from_image, from_html);
}

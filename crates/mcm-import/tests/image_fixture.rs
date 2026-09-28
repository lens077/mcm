//! End-to-end: a real archify screenshot (Go service layering) must come out
//! as the same structure a person reads from it.

use mcm_import::import_image;

const FIXTURE: &[u8] = include_bytes!("../fixtures/archify-go-service.png");

fn task_line<'a>(outline: &'a str, title: &str) -> &'a str {
    outline
        .lines()
        .find(|l| {
            l.trim_start()
                .trim_start_matches("- ")
                .starts_with(&format!("{title} #"))
        })
        .unwrap_or_else(|| panic!("no task titled {title:?} in\n{outline}"))
}

fn id_of(outline: &str, title: &str) -> String {
    let line = task_line(outline, title);
    let at = line.find(" #t").expect("id") + 2;
    line[at..].split_whitespace().next().unwrap().to_owned()
}

#[test]
fn archify_screenshot_becomes_grouped_outline_with_dependencies() {
    let imported = import_image(FIXTURE, "Go 服务分层").expect("import");
    let out = &imported.outline;

    // Every box, by title.
    for title in [
        "Consul",
        "backend/api/{svc}/v1",
        "Kratos Gateway",
        "internal/server",
        "internal/service",
        "internal/biz",
        "internal/data",
        "internal/data/models",
        "internal/conf/v1",
        "Config Center",
        "Dragonfly",
    ] {
        task_line(out, title);
    }
    assert_eq!(imported.report.nodes, 12, "{out}");
    assert_eq!(imported.report.groups, 1, "{out}");

    // The dashed frame groups the six internal/* packages.
    let group = out
        .lines()
        .find(|l| l.starts_with("- backend/services/"))
        .expect("group task");
    assert!(group.contains("强制边界"), "{group}");
    for title in [
        "internal/server",
        "internal/service",
        "internal/biz",
        "internal/data",
        "internal/conf/v1",
    ] {
        assert!(
            task_line(out, title).starts_with("  - "),
            "{title} should be nested:\n{out}"
        );
    }

    // Subtitles become notes.
    assert!(out.contains("> 服务注册 + 健康检查"), "{out}");
    assert!(out.contains("> domain + application"), "{out}");

    // Every arrow, with its direction.
    let arrows = [
        ("Kratos Gateway", "internal/server"),
        ("internal/server", "internal/service"),
        ("internal/service", "internal/biz"),
        ("internal/biz", "internal/data"),
        ("internal/data", "internal/data/models"),
        ("internal/data", "Dragonfly"),
        ("internal/server", "Consul"),
        ("backend/api/{svc}/v1", "internal/service"),
        ("internal/conf/v1", "internal/server"),
        ("Config Center", "internal/conf/v1"),
    ];
    for (from, to) in arrows {
        let pred = format!("<-{}", id_of(out, from));
        assert!(
            task_line(out, to).split_whitespace().any(|t| t == pred),
            "missing arrow {from} → {to}:\n{out}"
        );
    }
    // PostgreSQL's title may be read with a case slip ("PostgresQL"); the
    // arrow into it must still be there.
    let data = id_of(out, "internal/data");
    assert!(
        out.lines()
            .any(|l| l.starts_with("- Postgres") && l.contains(&format!("<-{data}"))),
        "{out}"
    );
    assert_eq!(imported.report.dependencies, 11, "{out}");
    assert_eq!(imported.report.undirected, 0);

    // Viewer toolbar is not a node; its text is kept as a comment.
    assert!(!out.lines().any(|l| l.starts_with("- PATH")), "{out}");
    assert!(out.contains("# 图中未归属的文字：Legend"), "{out}");

    // The outline is clean input for the regular pipeline.
    let parsed = mcm_core::outline::parse(out);
    assert!(
        parsed.issues.iter().all(|i| !i.is_error()),
        "{:?}",
        parsed.issues
    );
    let issues = mcm_core::validate::validate(&parsed.plan);
    assert!(issues.iter().all(|i| !i.is_error()), "{issues:?}");
    assert_eq!(parsed.plan.dependencies.len(), 11);
}

#[test]
fn import_is_deterministic() {
    let a = import_image(FIXTURE, "x").unwrap().outline;
    let b = import_image(FIXTURE, "x").unwrap().outline;
    assert_eq!(a, b);
}

#[test]
fn garbage_bytes_are_a_decode_error() {
    let err = import_image(b"not an image", "x").unwrap_err();
    assert!(matches!(err, mcm_import::ImportError::Decode(_)), "{err}");
}

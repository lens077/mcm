//! Performance budgets from plan.md / quickstart.md §性能预算断言.
//!
//! These are hard gates (宪法 II): the harness fails the run when a budget is
//! exceeded, so a regression cannot merge silently.
//!
//! ```text
//! cargo bench -p mcm-core
//! ```

use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use mcm_core::outline::{parse, serialize};
use mcm_core::scene::{ViewKind, scene};
use mcm_core::validate::validate;

fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/perf")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Runs `op` a few times and returns the best wall-clock duration, which is the
/// figure the budget is written against.
fn best_of(iterations: u32, mut op: impl FnMut()) -> Duration {
    let mut best = Duration::MAX;
    for _ in 0..iterations {
        let start = Instant::now();
        op();
        best = best.min(start.elapsed());
    }
    best
}

fn assert_budget(label: &str, actual: Duration, budget: Duration) {
    assert!(
        actual <= budget,
        "{label} exceeded its budget: {actual:?} > {budget:?} (see plan.md §Performance Goals)"
    );
    println!("budget ok: {label} {actual:?} <= {budget:?}");
}

fn budgets(c: &mut Criterion) {
    let big = fixture("plan-5000.mcm");
    let medium = fixture("plan-1000.mcm");

    // --- Budget assertions (hard gates) ------------------------------------
    // Debug/bench builds carry overhead, so budgets are checked on optimised
    // bench profile timings only.
    let parse_and_validate = best_of(5, || {
        let parsed = parse(black_box(&big));
        let issues = validate(&parsed.plan);
        black_box(issues);
    });
    assert_budget(
        "parse+validate 5000 tasks",
        parse_and_validate,
        Duration::from_millis(200),
    );

    let plan = parse(&medium).plan;
    let issues = validate(&plan);

    for view in [
        ViewKind::Wbs,
        ViewKind::DepGraph,
        ViewKind::Timeline,
        ViewKind::Milestones,
    ] {
        let elapsed = best_of(5, || {
            black_box(scene(black_box(&plan), view, black_box(&issues)));
        });
        assert_budget(
            &format!("scene_get {view:?} 1000 tasks"),
            elapsed,
            Duration::from_millis(50),
        );
    }

    let revalidate = best_of(5, || {
        black_box(validate(black_box(&plan)));
    });
    assert_budget(
        "revalidate 1000 tasks",
        revalidate,
        Duration::from_millis(50),
    );

    let round_trip = best_of(5, || {
        black_box(serialize(black_box(&plan)));
    });
    assert_budget(
        "serialize 1000 tasks",
        round_trip,
        Duration::from_millis(50),
    );

    // --- Criterion measurements (trend tracking) ---------------------------
    let mut group = c.benchmark_group("core");
    group.bench_function("parse_5000", |b| b.iter(|| parse(black_box(&big))));
    group.bench_function("validate_1000", |b| b.iter(|| validate(black_box(&plan))));
    group.bench_function("scene_wbs_1000", |b| {
        b.iter(|| scene(black_box(&plan), ViewKind::Wbs, black_box(&issues)))
    });
    group.bench_function("scene_depgraph_1000", |b| {
        b.iter(|| scene(black_box(&plan), ViewKind::DepGraph, black_box(&issues)))
    });
    group.bench_function("scene_timeline_1000", |b| {
        b.iter(|| scene(black_box(&plan), ViewKind::Timeline, black_box(&issues)))
    });
    group.bench_function("serialize_1000", |b| b.iter(|| serialize(black_box(&plan))));
    group.finish();
}

criterion_group!(benches, budgets);
criterion_main!(benches);

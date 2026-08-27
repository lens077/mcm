//! Generates deterministic performance fixtures.
//!
//! ```text
//! cargo run -p mcm-core --bin gen_fixture -- 1000 fixtures/perf/plan-1000.mcm
//! ```
//!
//! The output is a valid canonical outline so it doubles as a golden document
//! for the parser and every layout.

use std::fmt::Write as _;

/// Builds a plan of `task_count` tasks arranged as phases of ten tasks each,
/// with a dependency chain inside every phase and a milestone per phase.
#[must_use]
pub fn generate(task_count: usize) -> String {
    let mut out = String::new();
    out.push_str("%mcm 1\n%title 性能基准规划\n%start 2026-01-05\n\n");

    let phase_size = 10usize;
    let phases = task_count.div_ceil(phase_size);
    let mut id = 1usize;
    let mut milestone_id = 1usize;
    let mut phase_last_ids: Vec<usize> = Vec::new();

    for phase in 0..phases {
        if id > task_count {
            break;
        }
        let phase_root = id;
        let _ = writeln!(out, "- 阶段 {} #t{phase_root}", phase + 1);
        id += 1;

        let mut previous: Option<usize> = None;
        let mut last_in_phase = phase_root;
        for index in 0..phase_size.saturating_sub(1) {
            if id > task_count {
                break;
            }
            let current = id;
            let dep = previous.map(|p| format!(" <-t{p}")).unwrap_or_default();
            let _ = writeln!(
                out,
                "  - 阶段 {} 任务 {} #t{current} [2d]{dep}",
                phase + 1,
                index + 1
            );
            previous = Some(current);
            last_in_phase = current;
            id += 1;
        }
        phase_last_ids.push(last_in_phase);
    }

    // One milestone per phase, gated on that phase's final task.
    for (index, last) in phase_last_ids.iter().enumerate() {
        let _ = writeln!(
            out,
            "! 阶段 {} 完成 #m{milestone_id} [2027-12-31] <-t{last}",
            index + 1
        );
        milestone_id += 1;
    }
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let count: usize = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1000);
    let path = args
        .next()
        .unwrap_or_else(|| "fixtures/perf/plan-1000.mcm".to_owned());

    let text = generate(count);
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            eprintln!("failed to create {}: {error}", parent.display());
            std::process::exit(1);
        }
    }
    match std::fs::write(&path, &text) {
        Ok(()) => println!("wrote {path} ({count} tasks, {} bytes)", text.len()),
        Err(error) => {
            eprintln!("failed to write {path}: {error}");
            std::process::exit(1);
        }
    }
}

//! Boot-path probe for the cold-start budget.
//!
//! `scripts/measure-startup.mjs` spawns the release binary with `--boot-probe`
//! and times it (spec SC-003: ≤ 2 s at P95). It exercises exactly the paths the
//! UI needs before first paint — outline in, validated plan out, all four scene
//! projections — and nothing else.
//!
//! Why not reuse `--selftest`: the scenario suite is a growing checklist, and
//! scenarios like the screenshot import do hundreds of milliseconds of real
//! OCR. Timing the suite would fold that work into the startup budget, so the
//! budget must have its own minimal mode. The suite stays complete; the probe
//! stays about startup.

use mcm_core::scene::ViewKind;
use mcm_core::session::Session;

/// A small, healthy outline touching every boot path: nesting, dependencies
/// across branches, a milestone, and dates to derive. Same shape as the
/// selftest sample — a plan the validator accepts as-is.
const SAMPLE: &str = "%mcm 1
%title 启动探针
%start 2026-09-01

- 需求阶段 #t1 [2026-09-01..2026-09-10] @王芳
  - 用户访谈 #t2 [2026-09-01..2026-09-03]
  - 竞品分析 #t3 [2026-09-04..2026-09-06] <-t2
- 设计阶段 #t4 [2026-09-11..2026-09-20] <-t3
! 需求冻结 #m1 [2026-09-30] <-t4
";

/// Entry point for `--boot-probe`: exit 0 when the boot path is healthy.
pub fn main_boot_probe() -> ! {
    let mut session = Session::new();
    session.apply_outline_text(SAMPLE);

    let healthy = session.error_count() == 0
        && session.plan().tasks.len() == 4
        && session.plan().milestones.len() == 1
        && session.plan().dependencies.len() == 2
        && ViewKind::all()
            .iter()
            .all(|view| !session.scene(*view).nodes.is_empty());

    if healthy {
        std::process::exit(0);
    }
    eprintln!("启动探针失败：引导路径不健康");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probe_sample_is_a_healthy_plan() {
        let mut session = Session::new();
        session.apply_outline_text(SAMPLE);
        assert_eq!(session.error_count(), 0);
        assert_eq!(session.plan().tasks.len(), 4);
        assert_eq!(session.plan().milestones.len(), 1);
        assert_eq!(session.plan().dependencies.len(), 2);
        for view in ViewKind::all() {
            assert!(!session.scene(view).nodes.is_empty(), "{view:?} 为空");
        }
    }
}

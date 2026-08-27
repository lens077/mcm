//! Scripted scenario harness (research.md R8).
//!
//! `tauri-driver` has no macOS support, so the same scenario list that the
//! WebDriver suite runs on Windows is executed here against the real core and
//! reported through the process exit code. Both platforms therefore gate on one
//! shared checklist (宪法 I).
//!
//! ```text
//! mcm-app --selftest
//! ```

use std::fmt::Write as _;
use std::path::PathBuf;

use mcm_core::edit::EditCommand;
use mcm_core::model::{Schedule, TaskId};
use mcm_core::scene::ViewKind;
use mcm_core::session::Session;

/// One scenario from quickstart.md §端到端验证场景.
pub struct Scenario {
    pub id: &'static str,
    pub name: &'static str,
    run: fn(&mut Context) -> Result<(), String>,
}

/// Scratch space shared by the scenarios.
pub struct Context {
    dir: PathBuf,
}

impl Context {
    fn new() -> Result<Self, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("mcm-selftest-{stamp}"));
        std::fs::create_dir_all(&dir).map_err(|error| format!("无法创建临时目录：{error}"))?;
        Ok(Self { dir })
    }

    fn file(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const SAMPLE: &str = "%mcm 1
%title 自检规划
%start 2026-09-01

- 需求阶段 #t1 [2026-09-01..2026-09-10] @王芳
  - 用户访谈 #t2 [2026-09-01..2026-09-03]
  - 竞品分析 #t3 [2026-09-04..2026-09-06] <-t2
- 设计阶段 #t4 [2026-09-11..2026-09-20] <-t3
! 需求冻结 #m1 [2026-09-30] <-t4
";

fn check(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}

/// Scenario 1: outline in, validated plan out.
fn scenario_generate(_ctx: &mut Context) -> Result<(), String> {
    let mut session = Session::new();
    session.apply_outline_text(SAMPLE);
    check(session.plan().tasks.len() == 4, "任务数应为 4")?;
    check(session.plan().milestones.len() == 1, "里程碑数应为 1")?;
    check(session.error_count() == 0, "健康规划不应有校验错误")
}

/// Scenario 2: a cycle is reported with its full path.
fn scenario_cycle(_ctx: &mut Context) -> Result<(), String> {
    let mut session = Session::new();
    session.apply_outline_text("- 甲 #t1 <-t3\n- 乙 #t2 <-t1\n- 丙 #t3 <-t2\n");
    let issue = session
        .issues()
        .iter()
        .find(|issue| issue.code == "V-CYCLE")
        .ok_or("应报告 V-CYCLE")?;
    let path = issue.cycle_path.as_ref().ok_or("V-CYCLE 应附环路径")?;
    check(path.first() == path.last(), "环路径应闭合")
}

/// Scenario 3: every view projects the same plan.
fn scenario_views(_ctx: &mut Context) -> Result<(), String> {
    let mut session = Session::new();
    session.apply_outline_text(SAMPLE);
    for view in ViewKind::all() {
        let scene = session.scene(view);
        check(scene.view == view, "场景视图类型应匹配")?;
        if view != ViewKind::Milestones {
            check(!scene.nodes.is_empty(), "视图不应为空")?;
        }
    }
    Ok(())
}

/// Scenario 5: edit, then undo/redo precisely.
fn scenario_edit_undo(_ctx: &mut Context) -> Result<(), String> {
    let mut session = Session::new();
    session.apply_outline_text(SAMPLE);
    let before = session.outline_text();

    session
        .edit(&EditCommand::RenameTask {
            id: TaskId(1),
            title: "改名".into(),
        })
        .map_err(|error| format!("编辑失败：{error}"))?;
    session
        .edit(&EditCommand::SetSchedule {
            id: TaskId(2),
            schedule: Schedule::Duration { days: 2 },
        })
        .map_err(|error| format!("编辑失败：{error}"))?;
    check(session.outline_text() != before, "编辑应改变文档")?;

    session.undo().ok_or("撤销应可用")?;
    session.undo().ok_or("撤销应可用")?;
    check(session.outline_text() == before, "撤销后应完全还原")?;

    session.redo().ok_or("重做应可用")?;
    check(session.outline_text() != before, "重做应重新应用")
}

/// Scenario 6: save and reopen losslessly.
fn scenario_save_reopen(ctx: &mut Context) -> Result<(), String> {
    let path = ctx.file("selftest.mcm");
    let mut session = Session::new();
    session.apply_outline_text(SAMPLE);
    let before = session.outline_text();
    session
        .save(Some(&path))
        .map_err(|error| format!("保存失败：{error}"))?;
    check(!session.is_dirty(), "保存后不应为脏")?;

    let mut reopened = Session::new();
    reopened
        .open(&path)
        .map_err(|error| format!("打开失败：{error}"))?;
    check(reopened.outline_text() == before, "重开应无损还原")
}

/// Scenario 7: a damaged file still opens, with its bad lines quarantined.
fn scenario_recovery(ctx: &mut Context) -> Result<(), String> {
    let path = ctx.file("damaged.mcm");
    std::fs::write(&path, "%mcm 1\n- 正常 #t1\n@@@ 坏行 @@@\n- 也正常 #t2\n")
        .map_err(|error| format!("写入夹具失败：{error}"))?;

    let mut session = Session::new();
    session
        .open(&path)
        .map_err(|error| format!("损坏文件应可打开：{error}"))?;
    check(session.plan().tasks.len() == 2, "健康任务应保留")?;
    check(session.recovery_report().count() == 1, "坏行应被隔离")
}

/// Scenario 8: XMind export opens as a real archive.
fn scenario_export_xmind(ctx: &mut Context) -> Result<(), String> {
    let path = ctx.file("selftest.xmind");
    let mut session = Session::new();
    session.apply_outline_text(SAMPLE);
    let report = mcm_export::xmind::export(session.plan(), &path)
        .map_err(|error| format!("导出失败：{error}"))?;
    check(path.exists(), "导出文件应存在")?;
    check(report.mapped_total() > 0, "导出摘要应记录映射内容")?;

    let file = std::fs::File::open(&path).map_err(|error| format!("无法打开：{error}"))?;
    let archive = zip::ZipArchive::new(file).map_err(|error| format!("非法 ZIP：{error}"))?;
    check(archive.len() == 3, "XMind 包应含三个条目")
}

/// Scenario 9: Visio export produces a closed, glued package.
fn scenario_export_vsdx(ctx: &mut Context) -> Result<(), String> {
    let path = ctx.file("selftest.vsdx");
    let mut session = Session::new();
    session.apply_outline_text(SAMPLE);
    mcm_export::vsdx::export(session.plan(), &path)
        .map_err(|error| format!("导出失败：{error}"))?;
    check(path.exists(), "导出文件应存在")?;

    use std::io::Read as _;
    let file = std::fs::File::open(&path).map_err(|error| format!("无法打开：{error}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| format!("非法 OPC 包：{error}"))?;
    let mut page = String::new();
    archive
        .by_name("visio/pages/page1.xml")
        .map_err(|error| format!("缺少 page1.xml：{error}"))?
        .read_to_string(&mut page)
        .map_err(|error| format!("page1.xml 不可读：{error}"))?;
    check(page.contains("ToPart=\"3\""), "应写出动态粘连 Connect 行")?;
    // Geometry must be readable without evaluating Visio-internal functions,
    // otherwise third-party viewers draw nothing.
    check(!page.contains("_WALKGLUE"), "不应写入 Visio 内部函数")?;
    check(!page.contains("Master="), "形状不应依赖 master")
}

/// Scenario 11: closing with unsaved changes is flagged.
fn scenario_close_guard(_ctx: &mut Context) -> Result<(), String> {
    let mut session = Session::new();
    check(!session.is_dirty(), "新会话不应为脏")?;
    session.apply_outline_text("- 甲 #t1\n");
    check(session.is_dirty(), "编辑后应标记为脏")
}

/// The shared checklist, mirrored by the Windows WebDriver suite.
pub const SCENARIOS: &[Scenario] = &[
    Scenario {
        id: "S1",
        name: "生成与校验",
        run: scenario_generate,
    },
    Scenario {
        id: "S2",
        name: "循环依赖定位",
        run: scenario_cycle,
    },
    Scenario {
        id: "S3",
        name: "四视图投影",
        run: scenario_views,
    },
    Scenario {
        id: "S5",
        name: "编辑与撤销重做",
        run: scenario_edit_undo,
    },
    Scenario {
        id: "S6",
        name: "保存与无损重开",
        run: scenario_save_reopen,
    },
    Scenario {
        id: "S7",
        name: "损坏文件恢复",
        run: scenario_recovery,
    },
    Scenario {
        id: "S8",
        name: "导出 XMind",
        run: scenario_export_xmind,
    },
    Scenario {
        id: "S9",
        name: "导出 Visio",
        run: scenario_export_vsdx,
    },
    Scenario {
        id: "S11",
        name: "未保存关闭守卫",
        run: scenario_close_guard,
    },
];

/// Runs every scenario, returning a human-readable report and a pass flag.
#[must_use]
pub fn run_all() -> (String, bool) {
    let mut report = String::new();
    let mut failures = 0usize;

    let mut context = match Context::new() {
        Ok(context) => context,
        Err(error) => return (format!("自检无法启动：{error}\n"), false),
    };

    for scenario in SCENARIOS {
        match (scenario.run)(&mut context) {
            Ok(()) => {
                let _ = writeln!(report, "PASS {} {}", scenario.id, scenario.name);
            }
            Err(message) => {
                failures += 1;
                let _ = writeln!(report, "FAIL {} {} — {message}", scenario.id, scenario.name);
            }
        }
    }

    let _ = writeln!(
        report,
        "\n{} 个场景，{} 通过，{failures} 失败",
        SCENARIOS.len(),
        SCENARIOS.len() - failures
    );
    (report, failures == 0)
}

/// Entry point for `--selftest`: prints the report and exits with 0 or 1.
pub fn main_selftest() -> ! {
    let (report, passed) = run_all();
    print!("{report}");
    std::process::exit(if passed { 0 } else { 1 });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scenario_passes() {
        let (report, passed) = run_all();
        assert!(passed, "self-test scenarios failed:\n{report}");
    }

    #[test]
    fn the_report_names_every_scenario() {
        let (report, _) = run_all();
        for scenario in SCENARIOS {
            assert!(
                report.contains(scenario.id),
                "missing {} in report",
                scenario.id
            );
            assert!(report.contains(scenario.name), "missing {}", scenario.name);
        }
    }

    #[test]
    fn scenario_ids_are_unique() {
        let ids: std::collections::BTreeSet<&str> =
            SCENARIOS.iter().map(|scenario| scenario.id).collect();
        assert_eq!(ids.len(), SCENARIOS.len());
    }

    #[test]
    fn the_checklist_covers_the_quickstart_scenarios() {
        // quickstart.md §端到端验证场景: 1,3,5,6,7,8,9,11 are automatable.
        for required in ["S1", "S3", "S5", "S6", "S7", "S8", "S9", "S11"] {
            assert!(
                SCENARIOS.iter().any(|scenario| scenario.id == required),
                "missing scenario {required}"
            );
        }
    }
}

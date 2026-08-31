//! 把一份规划的真实会话状态与各视图场景图转储为 JSON。
//!
//! 用途：给宣传站点生成产品截图。截图必须反映真实渲染，所以数据出自
//! 与应用完全相同的核心代码路径（解析 → 校验 → 布局 → 场景投影），
//! 再由真实的前端渲染器绘制，而不是另画一套示意图。
//!
//!     cargo run -p mcm-core --example dump_scenes -- <输入.mcm> <输出目录>

use std::path::Path;

use mcm_core::scene::ViewKind;
use mcm_core::session::Session;

fn main() {
    let mut args = std::env::args().skip(1);
    let source = args.next().unwrap_or_else(|| {
        eprintln!("用法: dump_scenes <输入.mcm> <输出目录>");
        std::process::exit(2);
    });
    let out_dir = args.next().unwrap_or_else(|| "scene-dump".to_owned());
    let out = Path::new(&out_dir);
    std::fs::create_dir_all(out).expect("创建输出目录");

    let text = std::fs::read_to_string(&source).expect("读取输入文件");
    let mut session = Session::new();
    session.apply_outline_text(&text);

    // 会话状态：标题、任务数、问题数等，前端顶栏与状态栏要用
    let state = session.state(session.undo_depth(), session.redo_depth());
    write_json(&out.join("session.json"), &state);
    write_json(&out.join("issues.json"), &session.issues());
    write_json(
        &out.join("outline.json"),
        &serde_json::json!({ "text": session.outline_text() }),
    );

    // 四个视图各自的场景图
    for view in ViewKind::all() {
        let scene = session.scene(view);
        let name = format!("scene-{}.json", view_slug(view));
        write_json(&out.join(&name), &scene);
        println!(
            "  {name}: {} 节点 / {} 连线",
            scene.nodes.len(),
            scene.edges.len()
        );
    }

    println!(
        "已转储到 {} —— 任务 {} 个，问题 {} 条",
        out.display(),
        session.plan().tasks.len(),
        session.issues().len()
    );
}

fn view_slug(view: ViewKind) -> &'static str {
    match view {
        ViewKind::Wbs => "wbs",
        ViewKind::DepGraph => "graph",
        ViewKind::Timeline => "timeline",
        ViewKind::Milestones => "milestones",
    }
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) {
    let text = serde_json::to_string_pretty(value).expect("序列化");
    std::fs::write(path, text).unwrap_or_else(|e| panic!("写入 {}: {e}", path.display()));
}

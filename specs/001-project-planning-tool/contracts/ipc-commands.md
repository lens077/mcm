# Contract: 前后端命令面（Tauri IPC Commands）

**用途**: WebView 前端与 Rust 核心的唯一通信面。前端不持有可变模型状态——一切
变更经命令进入 `mcm-core`，返回校验结果与场景失效提示（宪法 IV）。
**落位**: `src-tauri/src/commands.rs`（注册）→ `mcm-core`/`mcm-export`（实现）；
前端绑定 `src/ipc/`（TypeScript 类型与本契约同步）。

## 通用约定

- 负载为 JSON，字段 snake_case；日期 `YYYY-MM-DD` 字符串。
- 错误统一信封：`{ "code": "E_*", "message": "...", "details"?: {...} }`；
  前端按 code 分支，message 直接可展示（中文）。
- 变更类命令返回 `ApplyResult`，前端据此增量刷新：

```json
{
  "revision": 42,
  "issues": [ { "severity": "error", "code": "V-CYCLE", "target": {"dep": ["t3","t7"]},
                "message": "…", "fix_hint": "…", "cycle_path": ["t3","t5","t7","t3"] } ],
  "dirty": true,
  "scene_stale": ["wbs", "dep_graph", "timeline", "milestones"],
  "undo_depth": 12, "redo_depth": 0
}
```

`revision` 单调递增；`scene_stale` 列出需重取场景的视图，前端仅对活动视图即时
`scene_get`，其余惰性。

## 命令清单

| 命令 | 入参 | 出参 | 说明 |
|------|------|------|------|
| `session_new` | — | `SessionState` | 新建空规划 |
| `session_open` | `{path}` | `SessionState`（含恢复问题） | 打开 `.mcm`；损坏→恢复语义 |
| `session_save` | `{path?}` | `{path, saved: true}` | 原子保存；无 path 且新文件 → `E_NEED_PATH`（前端弹保存框） |
| `session_state` | — | `SessionState` | `{path?, dirty, title, revision, counts}` |
| `outline_text_get` | — | `{text}` | 当前模型的规范化大纲全文（大纲编辑器回显） |
| `outline_text_apply` | `{text}` | `ApplyResult` | 全文重解析（= ReplaceFromOutline，单一撤销边界） |
| `edit_apply` | `{command: EditCommand}` | `ApplyResult` | 封闭命令集见 [data-model.md](../data-model.md)；非法目标 → `E_BAD_TARGET` |
| `undo` / `redo` | — | `ApplyResult` | 栈空 → no-op（`undo_depth`/`redo_depth` 告知可用性） |
| `scene_get` | `{view}` | `SceneGraph` | `wbs` \| `dep_graph` \| `timeline` \| `milestones`；节点/边为扁平数组（性能） |
| `issues_get` | — | `{issues[]}` | 当前问题集（问题面板） |
| `search` | `{query}` | `{matches: [{ref, title, snippet}]}` | 标题/备注/负责人子串匹配，文档序 |
| `export_precheck` | `{format}` | `{ok, error_count}` | 存在 Error 时前端必须先弹确认（spec FR-011） |
| `export_run` | `{format: "xmind"\|"vsdx", path}` | `ExportReport` | 见两份导出契约；I/O 失败 → `E_EXPORT_IO`（含重试指引） |
| `prefs_get` / `prefs_set` | `{...}` | `{...}` | 主题、最近文件、按文件视图状态（不入 `.mcm`） |
| `app_close_check` | — | `{dirty}` | 关闭前脏检查（FR-016 提示由前端呈现） |
| `diagram_import` | `{path, model?: "fast"\|"accurate"}` | `{outline, report: ImportReport, elapsed_ms}` | 把架构图图片（PNG/JPEG/WebP/BMP，本地 OCR）或 archify HTML（`.html`/`.htm`，读取语义标注）转为大纲文本；**不改动会话**，由前端决定是否载入为新规划。读不到文件 → `E_FILE_IO`；解码/识别失败、HTML 无 archify 标注 → `E_IMPORT`。`model` 只对图片生效，缺省为 `fast`；选 `accurate` 但模型未下载 → `E_MODEL_MISSING`。见 [docs/image-import.md](../../../docs/image-import.md) |
| `diagram_source_read` | `{path}` | `{text, name}` | 读取导入对话框选中的 Mermaid 源文件，供 webview 用 mermaid 库解析与渲染。**只接受** `.md`/`.markdown`/`.mmd`/`.mermaid`、≤ 4 MB 的 UTF-8 文本，不是通用读文件接口；`name` 为去扩展名的文件名。其他扩展名、超限、非 UTF-8 → `E_IMPORT`；读不到 → `E_FILE_IO` |
| `graph_import` | `{spec: GraphSpec, fallbackTitle}` | `{outline, report: ImportReport, elapsed_ms}` | 把 webview 已解析的图（Mermaid flowchart）转为大纲文本，映射规则与 `diagram_import` 相同；**不改动会话**。`GraphSpec = {title, nodes: [{id, label, detail[]}], groups: [{id, label, members[]}], edges: [{from, to, directed, label?}]}`，`spec.title` 为空时用 `fallbackTitle`。空图、连线引用不存在的 id、子图互相包含 → `E_IMPORT` |
| `ocr_model_status` | — | `OcrModelStatus` | `{accurate_installed, accurate_download_bytes, accurate_dir, downloading}` |
| `ocr_model_download` | — | `OcrModelStatus` | **唯一的联网命令**，仅由用户点击触发；逐文件校验 SHA-256，按镜像顺序回退。进度经事件 `ocr-model-download` `{done, total}` 推送。失败 → `E_DOWNLOAD`；重复触发 → `E_BUSY` |
| `ocr_model_remove` | — | `OcrModelStatus` | 删除已下载的高精度模型；下载中 → `E_BUSY` |

## 错误码

| code | 场景 |
|------|------|
| `E_NEED_PATH` | 保存新文件未提供路径 |
| `E_FILE_IO` | 读写失败（含权限、被占用；message 给出重试指引） |
| `E_VERSION_TOO_NEW` | `%mcm` 主版本高于支持（plan-file-format §版本策略） |
| `E_BAD_TARGET` | 编辑命令引用不存在元素 |
| `E_EXPORT_IO` | 导出目标不可写/被目标工具占用（spec Edge case） |
| `E_IMPORT` | 图片无法解码、OCR 推理失败、图中没有可识别内容，或 Mermaid 源文件 / 图结构不可用 |
| `E_MODEL_MISSING` | 选择了高精度模型，但模型未下载或校验不符 |
| `E_DOWNLOAD` | 所有下载源都失败（message 列出每个源的原因） |
| `E_BUSY` | 高精度模型正在下载，暂不能再次下载或删除 |
| `E_INTERNAL` | 其余内部错误（附诊断 details，日志落盘） |

## 性能预算（宪法 II，criterion + 前端计时守护）

- `edit_apply` / `undo` / `redo`（1,000 任务）：核心 ≤ 50ms，端到端 ≤ 100ms
- `scene_get`（1,000 任务）：≤ 50ms；`outline_text_apply`（5,000 行）：≤ 200ms
- 载荷体量超预算时的既定优化路径：场景图改传紧凑二进制（Tauri raw response），
  契约不变，仅编码层替换（记录于 research.md R9）。

## 契约测试要求

1. `src/ipc/` 的 TS 类型与本表逐命令比对的编译期断言（typegen 或手工镜像 + CI diff）。
2. Rust 侧每命令集成测试：正常路径 + 每个错误码至少一例。
3. `ApplyResult.scene_stale` 正确性：每类 EditCommand 断言其失效视图集合。

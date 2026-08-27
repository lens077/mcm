# Tasks: 项目规划桌面工具（生成、校验、交互编辑与可编辑导出）

**Input**: Design documents from `/specs/001-project-planning-tool/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/（5 份）, quickstart.md

**Tests**: 本特性测试任务为**必选**——宪法 VII 与各契约将单测/golden/属性/契约测试
明确定义为合入门槛（非可选 TDD 装饰）。测试任务紧随其守护的实现之后，合入前必须绿。

**Organization**: 任务按用户故事分组，每个故事可独立实现、独立测试、独立交付。

## Format: `[ID] [P?] [Story] Description`

- **[P]**: 可并行（不同文件、无未完成依赖）
- **[Story]**: 所属用户故事（US1–US6，映射 spec.md）
- 每个任务附准确文件路径

## Path Conventions

Tauri 单应用 + Rust workspace（见 plan.md 结构决策）：
`crates/mcm-core/`（领域核心）、`crates/mcm-export/`（导出器）、`src-tauri/`（壳）、
`src/`（React 19 前端）、`tests/e2e/`、`.github/workflows/`。

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: 仓库脚手架、双语言工程初始化、CI 骨架

- [X] T001 前端脚手架：`package.json`（pnpm scripts: dev/build/test/tauri）、`vite.config.ts`、`tsconfig.json`（strict 全开）、`index.html`、`src/app/main.tsx` 空壳渲染
- [X] T002 [P] Rust workspace：根 `Cargo.toml`（members = crates/mcm-core, crates/mcm-export, src-tauri）、`rust-toolchain.toml`（stable）、两个 crate 的空骨架 `crates/mcm-core/src/lib.rs`、`crates/mcm-export/src/lib.rs`
- [X] T003 Tauri 2.11 壳：`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`（窗口标题/最小尺寸/bundle 标识）、`src-tauri/src/main.rs`；`pnpm tauri dev` 打开空窗口即通过
- [X] T004 [P] 设计令牌与主题：`src/design/tokens.css`（颜色/字体/间距/圆角/阴影双主题，`data-theme` 切换）、`src/design/global.css`（去 WebView 默认样式）、`src/app/theme.ts`（主题读取/切换/持久化）——宪法 III：组件禁止硬编码样式值
- [X] T005 [P] CI 矩阵：`.github/workflows/ci.yml`（macos-latest + windows-latest：`cargo test --workspace`、`cargo clippy -- -D warnings`、`pnpm test`、`pnpm tauri build` 冒烟）——宪法 I/VII 合入门槛
- [X] T006 [P] Lint/format：`rustfmt.toml`、`.eslintrc.cjs` + `.prettierrc`（TS strict 规则）、`package.json` lint script 接入 CI

**Checkpoint**: 双平台 CI 绿、空窗口可开、主题变量就位

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: 所有故事共同依赖的模型、IPC 骨架与渲染底座

**⚠️ CRITICAL**: 本阶段未完成前不得开始任何用户故事

- [X] T007 核心模型类型：`crates/mcm-core/src/model/mod.rs` + `ids.rs`（TaskId/MilestoneId 分配器，会话内单调）+ `types.rs`（Plan/Task/Dependency/Milestone/Schedule/ValidationIssue/ElementRef，serde derive）——严格按 data-model.md
- [X] T008 日期与工作日算术：`crates/mcm-core/src/model/dates.rs`（ISO 日期、工作日加减/区间包络，周一至五规则）+ 同文件单元测试——契约 outline-grammar §时间推导 的底层
- [X] T009 会话容器：`crates/mcm-core/src/session.rs`（Plan + revision 单调递增 + dirty 标记 + 骨架 open/save 接口留空实现）
- [X] T010 IPC 骨架：`src-tauri/src/commands.rs`（错误信封 `E_*`、`ApplyResult` 结构、`Mutex<Session>` 状态、`session_new`/`session_state` 两命令注册进 `main.rs`）+ 前端绑定 `src/ipc/types.ts`、`src/ipc/client.ts`（与 contracts/ipc-commands.md 逐字段一致）
- [X] T011 [P] Canvas 渲染底座：`src/canvas/renderer.ts`（DPR 缩放、视口变换、平移/缩放手势、分层缓存、有限动效开关）、`src/canvas/hit.ts`（几何命中测试）、`src/canvas/scene-types.ts`（SceneGraph TS 镜像）
- [X] T012 应用壳布局：`src/app/App.tsx`（侧栏 + 主区 + 状态栏 + 视图标签骨架 + 空态页）、`src/app/shortcuts.ts`（快捷键注册表，mac/Win 修饰键适配）、接入 T004 主题切换

**Checkpoint**: 前端能经 IPC 拿到空会话状态并渲染空画布——用户故事可并行开工

---

## Phase 3: User Story 1 - 录入项目描述并生成经校验的规划 (Priority: P1) 🎯 MVP

**Goal**: 大纲文本 → 确定性解析 → 统一模型 → 全量校验（定位+修复指引）→ WBS 视图呈现

**Independent Test**: 粘贴 outline-grammar.md 示例 → WBS 树完整呈现、问题面板空；
注入环/日期矛盾/坏引用 → 对应 V-*/P-* 问题精确定位（spec US1 验收场景 1–4）

### Implementation for User Story 1

- [X] T013 [P] [US1] 大纲词法与 CST：`crates/mcm-core/src/outline/lexer.rs`（行分类、缩进计量、注解 token 切分、注释/备注归属预处理）——按 contracts/outline-grammar.md 行类型表
- [X] T014 [US1] 解析器 → 模型：`crates/mcm-core/src/outline/parser.rs`（指令、任务树、里程碑、`<-` 前置、`#id` 稳定分配与缺省补号、转义；P-001..P-008 含行列与 fix_hint）
- [X] T015 [US1] 规范化序列化器：`crates/mcm-core/src/outline/serialize.rs`（canonical 顺序、恒写 ID、注释/备注回写、转义补写、单尾换行）
- [X] T016 [US1] 往返契约测试：`crates/mcm-core/tests/outline_roundtrip.rs` + `crates/mcm-core/fixtures/outline/`（文法文档示例入 golden；属性测试 parse∘serialize 恒等；确定性 100 次重放；每个 P-* 最小触发例）——outline-grammar §契约测试 1–4
- [X] T017 [P] [US1] 校验引擎：`crates/mcm-core/src/validate/mod.rs`（V-REF/V-DUP/V-SELF/V-CYCLE 含完整环路径/V-HIER/V-RANGE/V-PARENT/V-ORDER/V-MSTONE/V-TITLE/W-NODATE/W-ORPHAN；每规则单元测试至少一个触发例，断言定位与 fix_hint 非空）
- [X] T018 [US1] 时间推导：`crates/mcm-core/src/validate/derive.rs`（V-CYCLE 通过后拓扑序单遍 effective_start/end，规则按 outline-grammar §时间推导）+ 单元测试（含工期跨周末例）
- [X] T019 [P] [US1] WBS 布局与场景投影：`crates/mcm-core/src/layout/wbs.rs`（整树布局，排序键稳定）+ `crates/mcm-core/src/scene/mod.rs`（SceneGraph 投影：节点几何/style_role/badges，扁平数组序列化）
- [X] T020 [US1] IPC 扩展：`src-tauri/src/commands.rs` 增加 `outline_text_get`/`outline_text_apply`/`scene_get(wbs)`/`issues_get` + `src/ipc/client.ts` 对应绑定与类型
- [X] T021 [P] [US1] 大纲编辑器面板：`src/panels/OutlineEditor.tsx`（行号、等宽编辑区、"生成"动作、P-* 错误行内标注与跳转）
- [X] T022 [US1] WBS 视图渲染：`src/views/wbs/WbsView.tsx`（消费场景图经 canvas 渲染节点/层级线/问题徽标，缩放平移接 T011）
- [X] T023 [US1] 问题面板：`src/panels/IssuesPanel.tsx`（严重级分组、点击定位到视图元素、展示 fix_hint；环路径完整展示）
- [X] T024 [US1] US1 验收集成测试：`crates/mcm-core/tests/us1_acceptance.rs`（spec US1 四个验收场景在模型层逐一断言：完整生成零问题、环报告含路径、父子日期冲突定位、坏引用定位）

**Checkpoint**: MVP 可演示——录入 → 生成 → 看树 → 看问题

---

## Phase 4: User Story 2 - 四种联动视图交互查看 (Priority: P1)

**Goal**: 依赖网络/时间线/里程碑三视图补齐 + 四视图选中联动、搜索、双主题、1000 任务流畅

**Independent Test**: 对已生成规划逐视图核对呈现与联动（quickstart 场景 3–4）；
`fixtures/perf/plan-1000.mcm` 缩放拖拽无可感知卡顿

### Implementation for User Story 2

- [X] T025 [P] [US2] 依赖图布局：`crates/mcm-core/src/layout/depgraph.rs`（最长路分层 → 重心法排序 → 折线路由；排序键稳定、确定性测试同文件）
- [X] T026 [P] [US2] 时间线布局：`crates/mcm-core/src/layout/timeline.rs`（日期标尺、泳道装箱、无日期任务分区 + W-NODATE 徽标）
- [X] T027 [P] [US2] 里程碑布局：`crates/mcm-core/src/layout/milestones.rs`（时间带排序 + 关联任务连线几何）
- [X] T028 [US2] 场景投影扩展三视图 + IPC：`crates/mcm-core/src/scene/mod.rs`（dep_graph/timeline/milestones 投影）+ `src-tauri/src/commands.rs` `scene_get` 全视图支持
- [X] T029 [US2] 三个视图组件：`src/views/graph/GraphView.tsx`、`src/views/timeline/TimelineView.tsx`、`src/views/milestones/MilestonesView.tsx`（均消费场景图 + T011 渲染底座）
- [X] T030 [US2] 跨视图选中联动：`src/app/selection.ts`（选中态单源 + 视图切换自动定位滚动，FR-007）接入四视图
- [X] T031 [US2] 搜索：`src-tauri/src/commands.rs` `search` 命令 + `src/panels/SearchBar.tsx`（高亮、逐个跳转，FR-008）
- [X] T032 [P] [US2] 全视图双主题样式角色：`src/design/tokens.css` 补齐全部 style_role 映射（节点/边/徽标/选中态双主题）+ 有限动效参数；开发用帧率浮层 `src/app/PerfOverlay.tsx`
- [X] T033 [P] [US2] 性能夹具与基准：`crates/mcm-core/src/bin/gen_fixture.rs`（生成 `fixtures/perf/plan-1000.mcm` 与 5000 行大纲）+ `crates/mcm-core/benches/core_budgets.rs`（criterion：解析 5000 行 ≤200ms、scene_get ≤50ms、单条编辑 ≤50ms，超预算 CI 失败）

**Checkpoint**: 四视图完整、联动、流畅——US1+US2 独立可测

---

## Phase 5: User Story 3 - 在视图中直接编辑并即时重校验 (Priority: P1)

**Goal**: 封闭命令集全类型编辑、增量重校验非阻断标注、精确撤销/重做

**Independent Test**: quickstart 场景 5——增删改/拖拽换父/连线成环标注/日期拖拽/
撤销重做链精确回放

### Implementation for User Story 3

- [X] T034 [US3] 编辑命令集：`crates/mcm-core/src/edit/commands.rs`（data-model.md 全部命令的 apply + 逆命令构造；DeleteTask 级联记录）
- [X] T035 [US3] 撤销日志：`crates/mcm-core/src/edit/journal.rs`（undo/redo 栈、新命令清 redo、ReplaceFromOutline 全文对单一边界、保存不截断）
- [X] T036 [US3] 增量重校验与场景失效：`crates/mcm-core/src/validate/incremental.rs`（按命令影响面重算问题子集）+ `src-tauri/src/commands.rs` `edit_apply`/`undo`/`redo` 返回 ApplyResult（issues/scene_stale/undo_depth）
- [X] T037 [P] [US3] WBS 直接编辑：`src/views/wbs/interactions.ts`（双击改名、Enter/Tab 新增子/同级、Del 删除确认、拖拽换父带落点指示）
- [X] T038 [P] [US3] 依赖图连线编辑：`src/views/graph/interactions.ts`（节点锚点拖出连线建依赖、选中边 Del 断开、成环即时红标 + 环路径悬浮提示）
- [X] T039 [P] [US3] 时间线日期编辑：`src/views/timeline/interactions.ts`（条形拖动平移/端点拖动改起止 → SetSchedule，冲突即时标注）
- [X] T040 [US3] 工具栏与撤销 UI：`src/app/Toolbar.tsx`（撤销/重做按钮态接 undo_depth、Cmd/Ctrl+Z 与 Shift+Z 快捷键接 T012 注册表）
- [X] T041 [P] [US3] 编辑核心测试：`crates/mcm-core/tests/edit_undo.rs`（随机命令序列 apply∘undo 恒等；跨 ReplaceFromOutline 边界回退；scene_stale 每命令类断言——ipc-commands §契约测试 3）

**Checkpoint**: 三个 P1 故事完成——工具日常可用

---

## Phase 6: User Story 4 - 本地保存与无损重开 (Priority: P1)

**Goal**: `.mcm` 原子保存、无损往返、行级恢复、未保存保护、外部修改提示

**Independent Test**: quickstart 场景 6–7 + 11——保存重开零丢失、手工编辑生效、
坏行隔离、关窗提示

### Implementation for User Story 4

- [X] T042 [US4] 原子保存与打开：`crates/mcm-core/src/session.rs` 补全（临时文件+fsync+rename、CRLF/无 BOM 容忍、`%mcm` 版本策略含 E_VERSION_TOO_NEW）——contracts/plan-file-format.md §原子保存/§版本策略
- [X] T043 [P] [US4] 恢复解析：`crates/mcm-core/src/outline/recover.rs`（坏行隔离为 P-* + 原文、保存回写 `# [mcm:recovered]` 注释、完全不可读文件拒开）——§恢复语义
- [X] T044 [US4] 文件命令与对话框：`src-tauri/src/commands.rs` `session_open`/`session_save`/`app_close_check` + Tauri dialog/fs 能力配置 `src-tauri/capabilities/default.json` + `src/app/App.tsx` 打开/保存菜单动作与未保存关窗三选提示（FR-016）
- [X] T045 [US4] 外部修改监测：`src-tauri/src/watch.rs`（窗口聚焦时 mtime 比对 → 事件推前端）+ `src/app/App.tsx` "重新加载/保留内存版本"提示——plan-file-format §外部修改
- [X] T046 [P] [US4] 偏好与最近文件：`src-tauri/src/commands.rs` `prefs_get`/`prefs_set`（应用数据目录 JSON）+ `src/app/recent.ts`（最近文件、按路径视图状态——不入 `.mcm`）
- [X] T047 [US4] 文件契约测试：`crates/mcm-core/tests/file_roundtrip.rs`（100 次随机编辑后往返相等 SC-006；rename 前崩溃注入原文件完好；`%mcm 2` 拒开；CRLF/乱序注解输入规范化）——plan-file-format §契约测试 1–5

**Checkpoint**: 四个 P1 故事完成——数据底座可信

---

## Phase 7: User Story 5 - 导出 XMind 并可继续编辑 (Priority: P2)

**Goal**: 生成 XMind 2020–2026 可开可编辑的 .xmind；依赖=真实 relationship 连线；
降级逐项入报告

**Independent Test**: quickstart 场景 8——契约测试绿 + XMind 实开编辑保存无报错

### Implementation for User Story 5

- [X] T048 [P] [US5] XMind 写入器：`crates/mcm-export/src/xmind/writer.rs`（ZIP STORE 三件套、content.json 顶层数组、任务树→attached、依赖→relationships、done→task-done、日期/负责人→labels、里程碑分支+flag-red、备注→notes.plain 尾 `\n`、确定性 UUID）——contracts/export-xmind.md 映射表逐行
- [X] T049 [US5] 导出报告与 IPC：`crates/mcm-export/src/report.rs`（mapped/degraded/warnings 构造）+ `src-tauri/src/commands.rs` `export_precheck`/`export_run(xmind)`（校验 Error 先确认，FR-011；E_EXPORT_IO 重试指引）
- [X] T050 [US5] 导出对话框：`src/panels/ExportDialog.tsx`（格式选择、目标路径、错误警示确认流、ExportReport 降级清单展示——SC-008 零静默）
- [X] T051 [P] [US5] XMind 契约测试：`crates/mcm-export/tests/xmind_contract.rs` + `crates/mcm-export/fixtures/xmind/content.schema.json`（结构断言/schema 校验/树同构/relationship 闭合/STORE 断言/降级完备计数/独立最小读取器回读）——export-xmind §契约测试 1–4
- [X] T052 [US5] XMind 导出自检：`crates/mcm-export/src/xmind/verify.rs`（落盘前重解包+schema+引用闭合，失败即导出失败不落半成品）——export-xmind §生成规则

**Checkpoint**: 第一种可编辑导出交付

---

## Phase 8: User Story 6 - 导出 Visio 并可继续编辑 (Priority: P2)

**Goal**: 生成 Visio 2016+ 无修复提示、形状可编辑、连线动态粘连的 .vsdx

**Independent Test**: quickstart 场景 9——契约测试绿 + Visio 实开拖动形状连线跟随

### Implementation for User Story 6

- [X] T053 [P] [US6] OPC 包写入器：`crates/mcm-export/src/vsdx/opc.rs`（`[Content_Types].xml` 首条目 + 精确 Override 类型、rel 链、无目录条目、无 BOM、UTF-8 声明）——contracts/export-vsdx.md §OPC 包结构
- [X] T054 [P] [US6] Dynamic connector master：`crates/mcm-export/fixtures/vsdx/dynamic-connector-master.xml`（按 research-vsdx §4 制作：BaseID/MatchByName/ObjType=2/GlueType=2）+ `crates/mcm-export/src/vsdx/masters.rs`（masters.xml + master1.xml 写入）
- [X] T055 [US6] 页面写入器：`crates/mcm-export/src/vsdx/page.rs`（依赖图布局坐标→英寸+Y 翻转；无 master 矩形内联 Geometry + 文本行降级；里程碑菱形；连接器实例 `_WALKGLUE`/`_XFTRIGGER`/`GUARD` 公式 + `<Connects>` FromPart 9/12→ToPart 3 双保险）——契约映射表逐行
- [X] T056 [US6] 文档组装与 IPC：`crates/mcm-export/src/vsdx/document.rs`（document.xml + StyleSheet 0 链 + pages.xml + docProps 存根）+ `export_run(vsdx)` 接入 `src-tauri/src/commands.rs` 与 T050 对话框 + 落盘前自检（重解包/良构/ID 与 rel 闭合）
- [X] T057 [US6] VSDX 契约测试：`crates/mcm-export/tests/vsdx_contract.rs` + `crates/mcm-export/fixtures/vsdx/golden-small/`（part↔ContentType 一一对应、rel 闭合、Shape ID 唯一、每依赖恰两行 Connect、Geometry 行序、golden 规范化 diff、`libvisio-rs` 读回计数）——export-vsdx §契约测试 1–4

**Checkpoint**: 两种可编辑导出全部交付——全部用户故事完成

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: 双平台冒烟、性能封口、无障碍与发布质量

- [X] T058 [P] Windows E2E 冒烟：`tests/e2e/wdio.conf.ts` + `tests/e2e/smoke.spec.ts`（quickstart 场景 1/3/5/6/8/9/11 脚本化）+ CI 接 tauri-driver——research.md R8
- [X] T059 [P] macOS 场景自检 harness：`src-tauri/src/selftest.rs`（`--selftest` 参数驱动同一场景清单、退出码报告，feature gate）+ `package.json` `smoke:mac` script + CI 接入
- [X] T060 [P] 冷启动预算：`scripts/measure-startup.mjs`（20 次取 P95 ≤2s，双平台 CI 探针）+ 按测量结果做懒加载优化（涉及 `src/app/main.tsx` 启动路径）——SC-003
- [X] T061 键盘可达性收口：`src/app/shortcuts.ts` 全表补齐（视图切换/搜索/编辑/撤销/导出）+ 快捷键速查对话框 `src/panels/ShortcutsHelp.tsx` + 面板 Tab 焦点顺序审查——宪法 III
- [X] T062 [P] 打包与体积：`src-tauri/tauri.conf.json` bundle 配置 + `src-tauri/icons/` 应用图标 + CI 安装包体积断言 ≤25MB + macOS universal 目标构建
- [X] T063 [P] 发布人工验收清单：`specs/001-project-planning-tool/release-checklist.md`（quickstart §发布前人工验收成文：XMind/Visio 真实应用双平台核验步骤与记录表）

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)** → **Foundational (Phase 2)** → 各用户故事 → **Polish (Phase 9)**
- Phase 2 完成前不得开始任何故事；Phase 9 需全部所选故事完成

### User Story Dependencies

- **US1 (P1)**: 仅依赖 Foundational——MVP 起点，无故事间依赖
- **US2 (P1)**: 依赖 US1（模型/解析/场景底座 T013–T020）
- **US3 (P1)**: 核心（T034–T037、T040–T041）仅依赖 US1；**T038 需 US2 的 T029（GraphView）、T039 需 T029 的 TimelineView**——建议顺序 US1 → US2 → US3
- **US4 (P1)**: 依赖 US1（序列化器 T015）；与 US2/US3 无依赖，可并行
- **US5 (P2)**: 依赖 US1（模型）；与 US2/US3/US4 无依赖，可并行
- **US6 (P2)**: 依赖 US1 + **US2 的 T025（依赖图布局，vsdx 页面坐标来源）**
- 每故事内：模型/核心 → IPC → UI → 测试收口；契约测试与实现同阶段完成、合入前必须绿

### Parallel Opportunities

- Phase 1：T002/T004/T005/T006 并行；Phase 2：T011 与 T007–T010 双轨并行
- US1 内：T013（大纲）、T017（校验）、T019（布局）、T021（编辑器 UI）四轨并行
- US1 完成后（若多人）：A→US2、B→US4、C→US5 三线并行；US6 待 T025 后加入
- Polish：T058/T059/T060/T062/T063 全部并行

---

## Parallel Example: User Story 1

```bash
# Foundational 完成后同时启动四轨：
Task: "T013 大纲词法与 CST in crates/mcm-core/src/outline/lexer.rs"
Task: "T017 校验引擎 in crates/mcm-core/src/validate/mod.rs"
Task: "T019 WBS 布局与场景投影 in crates/mcm-core/src/layout/wbs.rs + scene/mod.rs"
Task: "T021 大纲编辑器面板 in src/panels/OutlineEditor.tsx"
# 汇合点：T020（IPC 扩展）需要 T014/T017/T019；T022–T024 随后
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1 Setup → Phase 2 Foundational（阻塞项）
2. Phase 3 US1 完成 → **停下验证**：quickstart 场景 1–2 独立走通
3. 此时即为可演示 MVP：录入 → 生成 → 校验 → WBS 视图

### Incremental Delivery

1. + US2 → 四视图完整（quickstart 场景 3–4）→ 可交付
2. + US3 → 可编辑（场景 5）→ 可交付
3. + US4 → 数据可信（场景 6–7、11）→ 首个"日常可用"版本
4. + US5 → XMind 导出（场景 8）→ 可交付
5. + US6 → Visio 导出（场景 9）→ 特性完成
6. Phase 9 → 双平台冒烟/预算/无障碍收口 → 发布候选

### Parallel Team Strategy

多人协作时：共同完成 Setup + Foundational → US1 全员合力（MVP 最快落地）→
之后 US2 / US4 / US5 三线并行，US3 随 US2 完成接续，US6 待 T025 就绪启动。

---

## Notes

- [P] = 不同文件且无未完成依赖；同文件任务（如多次触碰 `commands.rs`）已排为串行
- 全部 63 个任务均含准确文件路径；契约测试任务与其契约文档条款一一对应
- 每完成一个任务或逻辑组提交一次；每个 Checkpoint 可独立验收
- 性能预算（T033/T060）与契约测试同为 CI 阻断项——宪法 II/VI/VII

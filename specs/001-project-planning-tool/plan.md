# Implementation Plan: 项目规划桌面工具（生成、校验、交互编辑与可编辑导出）

**Branch**: `001-project-planning-tool` | **Date**: 2026-08-26 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-project-planning-tool/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

## Summary

用户以结构化大纲录入项目描述，Rust 核心经确定性解析生成统一规划模型（任务层级、
依赖、里程碑、时间线），校验通过后投影为四种联动视图（Canvas 渲染），支持视图内
直接编辑、即时重校验与完整撤销/重做；原生文件即大纲文本本身（人类可读、可 diff），
并由自研导出器生成 XMind（ZIP+JSON）与 Visio（OPC+XML）可继续编辑的文件。

技术路线：Tauri 2.11.x 桌面壳 + Rust workspace（`mcm-core` 领域核心、`mcm-export`
导出器）+ React 19 / TypeScript strict 前端。解析、校验、布局、撤销、导出全部在
Rust 侧完成；前端只做 Canvas 场景渲染与操作面板。视图渲染是模型场景图的纯投影。

## Technical Context

**Language/Version**: Rust stable（edition 2024）核心；TypeScript 5.x（strict）前端

**Primary Dependencies**: Tauri 2.11.x（壳/IPC/打包）；React 19.2.x + Vite（前端
chrome）；Rust 侧 `serde`/`serde_json`、`zip`、`quick-xml`（导出器）、`thiserror`；
四视图渲染为自研 Canvas 2D 渲染器（无重型图表库）

**Storage**: 本地文件——原生格式 `.mcm`（大纲文本，UTF-8，含版本头）；应用偏好
（主题、最近文件、视图状态）存应用数据目录 JSON；无数据库、无云端

**Testing**: `cargo test`（模型单测、解析 golden 测试、校验属性测试、导出契约
测试）；`criterion` 性能基准；Vitest（前端单元）；WebdriverIO + `tauri-driver`
（Windows E2E 冒烟）；macOS 冒烟经内置场景自检 harness（`tauri-driver` 不支持
macOS，见 research.md R8）

**Target Platform**: macOS 10.15+（x86_64 + aarch64 universal）；Windows 10+
（WebView2）

**Project Type**: desktop-app（Tauri：Rust workspace + Web 前端）

**Performance Goals**: 冷启动可交互 ≤ 2s（P95）；编辑反馈 ≤ 100ms；1,000 任务
视图缩放/拖拽 ≥ 60fps；5,000 行大纲解析+校验 ≤ 200ms；1,000 任务导出 ≤ 3s

**Constraints**: 核心功能完全离线；空闲内存 ≤ 200MB；安装包 ≤ 25MB；原生格式
必须人类可读可手工编辑；解析/布局/渲染全链路确定性（同输入必同输出）

**Scale/Scope**: 单用户单机；规划规模设计上限 ~5,000 任务（性能预算基线 1,000）；
4 种视图；2 种导出格式；首版界面简体中文

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | 宪法原则 | 本计划的符合方式 | 结论 |
|---|---------|----------------|------|
| I | 跨平台桌面一致性 | CI 矩阵同时构建 macOS + Windows 并跑冒烟；快捷键/菜单经平台适配层，能力集合无平台缺口；平台差异记录于 quickstart | PASS |
| II | 性能即功能 | 解析、校验、布局、撤销、导出全部在 Rust 核心；前端仅按场景图作 Canvas 渲染（视口裁剪+分层缓存）；预算落为 criterion 基准与启动探针，CI 阻断回归 | PASS |
| III | 界面美观与体验标准 | design tokens 单源（`src/design/tokens.css`）生成深浅主题；无硬编码样式值；动效有限且可全局关闭；核心操作键盘可达 | PASS |
| IV | 结构化模型先行、校验后渲染 | 模型唯一存于 Rust `mcm-core`；一切变更走封闭 EditCommand 集 → 重校验 → 场景图投影；渲染是场景图纯函数，前端无影子状态 | PASS |
| V | 人类可编辑、无锁定的数据 | 原生格式 = 大纲文本本身（`.mcm`），带 `%mcm 1` 版本头；规范化序列化保留注释；行级恢复解析器（contracts/plan-file-format.md） | PASS |
| VI | 导出保真 | 自研结构化导出器：.xmind（ZIP+content.json）、.vsdx（OPC+XML，Connect 粘连）；每格式契约测试（解包+schema/结构断言+降级清单断言）；降级报告 UI 呈现 | PASS |
| VII | 测试与质量门 | cargo 单测/golden/属性测试 + 导出契约测试 + Vitest + 双平台冒烟全部为合入门槛；缺陷先补复现测试 | PASS |
| — | 技术栈约束 | Tauri 2.x ✓；TS strict ✓；前端框架在本计划选定并锁定：React 19（research.md R2）✓；macOS 10.15+/Win10+ ✓；离线默认 ✓；lockfile 提交 ✓ | PASS |

**Initial Constitution Check: PASS（无违规）** → 进入 Phase 0。

**Post-Design Re-check（Phase 1 完成后复查）: PASS** ——设计产物未引入任何偏离：
模型/校验/布局/导出全部落位 `mcm-core`/`mcm-export`（II、IV）；文件契约即大纲
文本（V）；两份导出契约含逐条降级规则与契约测试断言（VI）；无需填写复杂度追踪表。

## Project Structure

### Documentation (this feature)

```text
specs/001-project-planning-tool/
├── plan.md              # 本文件（/speckit-plan 输出）
├── research.md          # Phase 0 输出（技术决策 R1–R11）
├── research-xmind.md    # Phase 0 附件：XMind 格式研究报告（子代理产出）
├── research-vsdx.md     # Phase 0 附件：VSDX 格式研究报告（子代理产出）
├── data-model.md        # Phase 1 输出（实体、校验规则、状态转换）
├── quickstart.md        # Phase 1 输出（环境、运行、验证场景）
├── contracts/           # Phase 1 输出
│   ├── outline-grammar.md    # 大纲语言文法（输入与文件的共同核心）
│   ├── plan-file-format.md   # .mcm 文件契约（版本、规范化、恢复）
│   ├── ipc-commands.md       # 前后端命令面（Tauri commands）
│   ├── export-xmind.md       # 模型 → .xmind 映射与契约测试断言
│   └── export-vsdx.md        # 模型 → .vsdx 映射与契约测试断言
└── tasks.md             # Phase 2 输出（/speckit-tasks 生成，非本命令产物）
```

### Source Code (repository root)

```text
mcm/
├── src-tauri/                    # Tauri 壳（薄层）
│   ├── src/main.rs               # 窗口、菜单、生命周期
│   ├── src/commands.rs           # IPC 命令注册 → 转发 mcm-core/mcm-export
│   ├── tauri.conf.json
│   └── Cargo.toml
├── crates/
│   ├── mcm-core/                 # 领域核心（纯 Rust、无 Tauri 依赖、可独立测试）
│   │   └── src/
│   │       ├── model/            # Plan/Task/Dependency/Milestone/日期与 ID
│   │       ├── outline/          # 词法/语法解析、CST、规范化序列化、恢复解析
│   │       ├── validate/         # V-* 规则引擎（引用/环/日期/层级）
│   │       ├── layout/           # 四视图布局：WBS 树、依赖 DAG 分层、时间线、里程碑
│   │       ├── scene/            # SceneGraph 投影（视图无关几何 + 样式角色）
│   │       ├── edit/             # EditCommand 封闭集 + 撤销/重做日志
│   │       └── session.rs        # 打开/保存/脏标记/原子写/恢复
│   └── mcm-export/               # 导出器（依赖 mcm-core 模型）
│       └── src/
│           ├── xmind/            # ZIP + content.json 生成
│           ├── vsdx/             # OPC 包 + page XML + Connect 粘连生成
│           └── report.rs         # ExportReport（映射/降级清单）
├── src/                          # 前端（React 19 + TS strict + Vite）
│   ├── design/                   # tokens.css（深浅主题单源）、全局样式
│   ├── ipc/                      # 类型化命令绑定（与 contracts/ipc-commands.md 同步）
│   ├── canvas/                   # 通用 Canvas 渲染器、命中测试、视口、动效
│   ├── views/                    # wbs/ graph/ timeline/ milestones/ 四视图
│   ├── panels/                   # 大纲编辑器、问题面板、导出对话框、搜索
│   └── app/                      # 布局壳、主题切换、快捷键、菜单联动
├── tests/
│   └── e2e/                      # WebdriverIO 冒烟（Win）+ 场景自检 harness（mac）
├── .github/workflows/ci.yml      # macOS + Windows 矩阵：构建/测试/契约/预算
├── package.json / pnpm-lock.yaml
└── Cargo.toml                    # workspace 根
```

**Structure Decision**: 采用 Tauri 单应用结构 + Rust workspace 双 crate。
`mcm-core` 不依赖 Tauri，保证核心可独立 `cargo test`（宪法 II/VII）；
`mcm-export` 单列使导出契约测试与 fixtures 内聚；`src-tauri` 仅做命令转发与
窗口壳。前端按"渲染层（canvas/views）与 chrome（panels/app）"分离，
渲染层只消费场景图。

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

无违规——本表为空。

<!--
Sync Impact Report
- Version change: (unfilled template) → 1.0.0 (initial ratification)
- Modified principles: all template placeholders replaced by 7 concrete principles:
  I. 跨平台桌面一致性 / II. 性能即功能 (NON-NEGOTIABLE) / III. 界面美观与体验标准 /
  IV. 结构化模型先行、校验后渲染 / V. 人类可编辑、无锁定的数据 /
  VI. 导出保真 (NON-NEGOTIABLE) / VII. 测试与质量门
- Added sections: 技术栈与平台约束（Technology & Platform Constraints）;
  开发工作流与质量门（Development Workflow & Quality Gates）
- Removed sections: none
- Templates: none modified — plan/spec/tasks templates read this constitution at runtime
  (per speckit-constitution scope guard).
- Deferred items:
  - TODO(PROJECT_NAME): 项目名 "MCM" 暂取自仓库目录名 mcm；确定正式产品名后以 PATCH 修订。
-->

# MCM Constitution

MCM 是一款面向开发者与产品经理的桌面端项目规划工具：将项目描述转化为经校验的、
精美可交互的规划视图（行为范式对标 archify），并可导出为 XMind、Visio 等
项目管理工具可继续编辑的文件格式。

## Core Principles

### I. 跨平台桌面一致性（Cross-Platform Desktop Parity）

macOS 与 Windows 是同等的一级目标平台。任何特性必须在两个平台上均可用且行为等价，
仅在两个平台都通过验证后才视为完成。交互细节（菜单、快捷键、窗口控制、文件对话框）
必须遵循各自平台惯例，但能力集合不得有平台缺口；确需平台特定行为时，必须在设计文档中
记录差异及理由。CI 必须同时构建并冒烟测试 macOS 与 Windows 两个产物。

**理由**：目标用户（开发者、产品经理）分布在两种系统上，任何单平台倾斜都会直接
削减一半用户的可用性。

### II. 性能即功能（Performance is a Feature，NON-NEGOTIABLE）

高性能是硬约束而非优化项。解析、布局计算、导出生成、文件 IO 等重计算必须在 Rust
核心中完成，WebView 前端只负责视图呈现与轻量交互。性能预算：

- 冷启动至可交互 ≤ 2s（P95）
- 常规交互反馈 ≤ 100ms；输入无可感知延迟
- 1,000 节点规模的规划视图渲染、缩放、拖拽保持 ≥ 60fps
- 空闲内存占用 ≤ 200MB；安装包 ≤ 25MB

任何使预算回归的变更必须先优化；无法优化时必须在评审中书面论证并获得批准。

**理由**：选择 Tauri/Rust 的核心动机即性能与轻量，预算不可量化就不可守护。

### III. 界面美观与体验标准（UI Excellence）

UI 是本产品的核心竞争力，不是附属品。必须建立统一设计系统：颜色、字体、间距、
圆角、阴影全部来自设计令牌（design tokens），禁止组件内硬编码样式值。深色与浅色
主题为一等公民，所有视图必须在两种主题下验证。动效必须有限、有目的且可全局关闭。
核心操作必须键盘可达。不得残留系统 WebView 默认样式。

**理由**："UI 美观"是明确的产品要求；没有设计系统约束，美观无法在迭代中存续。

### IV. 结构化模型先行、校验后渲染（Validated Model First）

一切规划内容（任务、里程碑、依赖、负责人、时间线、备注）必须先落为带 schema 的
结构化模型（typed JSON），并通过确定性校验——引用完整性、依赖无环、日期一致性——
之后才允许渲染。渲染是模型的纯函数：同一模型必然产出同一视图。UI 展示的每一条
信息都必须可回溯到模型中的事实，禁止渲染层编造或推断模型中不存在的内容。

**理由**：这是 archify 行为范式的核心（typed IR + deterministic checks），
也是导出保真与视图多样化（大纲/脑图/时间线）能共存的唯一基础。

### V. 人类可编辑、无锁定的数据（Human-Editable Data, No Lock-In）

原生文件格式必须是人类可读、可手工编辑、可版本控制（可 diff、可合并）的文本格式。
禁止不透明的私有二进制格式。格式必须带版本号并附 schema；文件部分损坏时必须尽力
恢复其余内容而非整体拒开。用户数据永远属于用户：不得以格式为手段制造迁移壁垒。

**理由**：目标用户是开发者与产品经理，"人类可编辑的友好方式"是明确产品要求；
文本格式同时使规划文件可进入 Git 工作流。

### VI. 导出保真（Export Fidelity，NON-NEGOTIABLE）

导出到 XMind（.xmind）、Visio（.vsdx）等外部工具时，必须产出目标工具能打开并
**继续编辑**的结构化文件；禁止以位图截图或不可编辑对象冒充导出结果。每种导出格式
必须有契约测试：结构/schema 断言，加上在目标工具或其格式校验器中的可打开性验证。
目标格式无法表达的信息必须显式降级并在导出时告知用户，禁止静默丢失。

**理由**：可再编辑导出是本产品区别于纯绘图工具的核心承诺，保真度必须可测试。

### VII. 测试与质量门（Quality Gates）

Rust 核心的模型、校验、布局与导出逻辑必须有单元测试；模型 schema 与每种导出格式
必须有契约测试；关键用户路径（新建 → 编辑 → 保存 → 重开 → 导出）必须有跨平台
冒烟测试。上述测试与性能预算检查均为合入门槛：不绿不合。缺陷修复必须先补上能
复现该缺陷的测试。

**理由**：格式契约与跨平台行为靠人工回归无法守住，必须由自动化门槛承担。

## 技术栈与平台约束（Technology & Platform Constraints）

- **桌面框架**：Tauri 2.x + Rust（stable toolchain）。重逻辑在 Rust 核心 crate 中，
  与 Tauri 壳解耦，保证核心可独立测试。
- **前端**：TypeScript（strict 模式）。具体前端框架在 plan 阶段选定，一经选定即锁定，
  更换须修订本宪法所辖的 plan 记录。
- **目标系统**：macOS 10.15+（Intel 与 Apple Silicon 双架构）；Windows 10+（WebView2）。
- **本地优先**：核心功能必须完全离线可用；用户数据默认只存本地；任何网络能力必须是
  显式可选项，且默认关闭。
- **依赖治理**：Cargo.lock 与前端 lockfile 必须提交；新增依赖须在评审中说明用途。

## 开发工作流与质量门（Development Workflow & Quality Gates）

- 特性开发遵循 Spec Kit 流程：`/speckit.specify` → `/speckit.clarify` →
  `/speckit.plan` → `/speckit.tasks` → `/speckit.implement`；plan 阶段的
  Constitution Check 未通过不得进入实现。
- 代码评审必须核对宪法合规，重点核对原则 II（性能预算）、IV（模型先行）、
  VI（导出保真）。
- CI 门槛：macOS + Windows 双平台构建、全部测试、导出契约测试、性能预算检查，
  全绿方可合入。
- 产品版本采用 MAJOR.MINOR.PATCH；原生文件格式发生不兼容变更时必须提供自动迁移，
  且产品 MAJOR 版本号递增。

## Governance

本宪法优先于其他一切开发实践与个人偏好。

- **修订程序**：任何修订以 PR 形式提出，必须说明动机、影响范围与迁移措施；
  合并后更新版本号与 Last Amended 日期。
- **版本策略**：宪法版本遵循语义化版本——MAJOR：删除或不兼容地重定义原则；
  MINOR：新增原则/章节或实质性扩充；PATCH：措辞澄清与非语义修正。
- **合规审查**：每个 PR 与每次 `/speckit.plan` 的 Constitution Check 都必须对照
  本宪法核验；任何偏离必须在 plan 的 Complexity Tracking 中记录理由，无法证成
  则必须简化方案。
- **运行时指引**：Agent 开发上下文文件（如 CLAUDE.md 等）从本宪法派生，
  与本宪法冲突时以本宪法为准。

**Version**: 1.0.0 | **Ratified**: 2026-08-26 | **Last Amended**: 2026-08-26

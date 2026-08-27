# Research: 项目规划桌面工具（Phase 0）

**Feature**: [spec.md](./spec.md) | **Date**: 2026-08-26
Technical Context 中无遗留 NEEDS CLARIFICATION；本文逐项记录关键技术决策。
生态时效核查（2026-08）：Tauri 2.11.x 为当前稳定线（[Tauri releases](https://v2.tauri.app/release)），
React 19.2.x 为当前稳定版（[React versions](https://react.dev/versions)）。

## R1 桌面框架：Tauri 2.11.x

- **Decision**: Tauri 2.11.x（宪法既定 Tauri 2.x，锁定当前小版本线）。
- **Rationale**: Rust 核心与壳同语言零边界；安装包与内存远小于 Electron，可守住
  ≤25MB / ≤200MB 预算；WebView 前端满足"UI 美观"的设计系统诉求。
- **Alternatives considered**: Electron（包体/内存超预算一个数量级）；Qt/Slint/egui
  （原生渲染但设计系统迭代效率与 Web 生态差距大，egui 即时模式风格受限）；
  Flutter（与 Rust 核心割裂，双技术栈）。

## R2 前端框架：React 19 + Vite（就此锁定）

- **Decision**: React 19.2.x + TypeScript strict + Vite；宪法要求"plan 阶段选定并
  锁定"，本条即锁定记录。
- **Rationale**: 框架只承担 chrome（面板/对话框/主题），重渲染在 Canvas 层，框架
  运行时性能非瓶颈；React 生态（无障碍组件、测试工具、人才）最厚。
- **Alternatives considered**: Svelte 5（体积更小但生态与组件积累少）；SolidJS
  （性能优势在本架构中用不上）。

## R3 视图渲染：自研 Canvas 2D 渲染器

- **Decision**: 四视图共用一个自研 Canvas 2D 渲染层（视口裁剪、脏区/分层缓存、
  基于场景图几何的命中测试）；动效有限且可全局关闭。
- **Rationale**: 1,000 节点 + 依赖边的 60fps 目标下，SVG/DOM 节点数是明确风险；
  Canvas 对样式与主题的像素级控制满足宪法 III；场景图（几何+样式角色）由 Rust
  预计算，前端渲染是纯投影（宪法 IV）。
- **Alternatives considered**: SVG DOM（万级元素帧率风险）；WebGL（v1 复杂度不成
  比例）；现成图库 react-flow / mermaid / gantt 库（样式锁定、性能天花板、依赖重）。

## R4 原生文件格式：大纲文本即文件（`.mcm`）

- **Decision**: 原生格式 = 大纲文法全文 + `%mcm 1` 版本头；规范化序列化保留注释；
  详见 [contracts/plan-file-format.md](./contracts/plan-file-format.md)。
- **Rationale**: 把"录入语言"和"磁盘格式"统一成同一文法——所见即所存；行级 diff
  天然友好（宪法 V）；行级恢复语义直接可实现（FR-015）。
- **Alternatives considered**: JSON（机器友好但手写体验差）；YAML（缩进歧义与
  隐式类型坑）；Markdown+front-matter（任务注解语义无标准，解析歧义更大）。

## R5 XMind 导出：ZIP(STORE) + content.json 三件套

- **Decision**: 按官方 xmind-generator 序列化器为基准：`content.json`（顶层数组）
  + `metadata.json`（dataStructureVersion "2"）+ `manifest.json`，条目 STORE 不压缩；
  依赖映射为 sheet 级 relationship 真实连线。
- **Rationale**: 官方生成器即最小可开集合，2020→2026 读取路径稳定；Rust 无现成
  生成库（[检索确认](https://crates.io/crates/libvisio-rs)仅有 Visio 解析库）。
- **Alternatives considered**: 复用 xmind-sdk-js 经 Node 子进程（引入运行时依赖，
  违背离线轻量）；导出 FreeMind 等中转格式（不满足"XMind 可继续编辑"的直接承诺）。
- 细节与坑：[research-xmind.md](./research-xmind.md)；契约：
  [contracts/export-xmind.md](./contracts/export-xmind.md)。

## R6 Visio 导出：自研 OPC 包生成器（`zip` + `quick-xml`）

- **Decision**: 直接生成 .vsdx OPC 包：`[Content_Types].xml`、rels、document/pages
  XML。结论（依 [research-vsdx.md](./research-vsdx.md) 实证）：任务矩形**不用
  master**（Visio 自身即以内联 Geometry 保存且完全可编辑）；连接器实例化仓库自维护
  的单一 Dynamic connector master；粘连采用"双保险"——`_WALKGLUE`/`_XFTRIGGER`
  公式 + `<Connect>` 行（FromPart 9/12 → ToCell PinX/ToPart 3 动态粘连）。
  契约：[contracts/export-vsdx.md](./contracts/export-vsdx.md)。
- **Rationale**: Rust 生态无 vsdx 生成 crate（libvisio-rs 仅解析）；模板改写方案
  （python-vsdx 风格）引入外部运行时且难以满足契约测试的从零确定性生成。
- **Alternatives considered**: 嵌入 Python/`vsdx` 库（运行时依赖+分发复杂）；
  导出 SVG 让 Visio 导入（导入后非结构化形状、粘连丢失，违背 FR-019/020）。

## R7 布局引擎：Rust 自研三算法

- **Decision**: WBS 树 = 简化 Reingold–Tilford 整树布局；依赖图 = 轻量 Sugiyama
  （最长路分层 → 重心法少轮排序 → 折线路由）；时间线 = 日期标尺 + 泳道装箱；
  里程碑 = 时间带排序。全部输出进场景图，排序键稳定保证确定性。
- **Rationale**: 布局属重计算，宪法 II 要求落在 Rust；自研可控确定性与美学细节；
  1,000 节点规模三算法均为毫秒级。
- **Alternatives considered**: ELK/dagre（JS 实现，布局将回流 WebView 违背宪法 II）；
  graphviz 绑定（C 依赖、样式控制弱、输出非确定性风险）。

## R8 跨平台 E2E：tauri-driver（Win）+ 内置场景自检（mac）

- **Decision**: Windows CI 用 WebdriverIO + `tauri-driver`（2.0.x）跑冒烟；macOS
  因 `tauri-driver` 不支持 → debug 构建内置场景 harness（启动参数触发脚本化命令
  序列，断言后以退出码报告），两平台冒烟覆盖同一场景清单（quickstart §场景）。
- **Rationale**: 宪法 I 要求双平台门槛；此组合让 mac 冒烟不依赖不存在的驱动。
- **Alternatives considered**: 仅 Windows E2E（mac 出现平台缺口，违宪）；图像对比
  自动化（脆弱，作为发布前人工清单补充而非门槛）。

## R9 IPC 编码：JSON 先行，预留二进制逃生门

- **Decision**: 命令面 JSON（snake_case）；场景图用扁平数组；若 1,000 任务场景
  传输超预算，切换 Tauri raw response 二进制编码，契约形状不变。
- **Rationale**: 先简单可调试；预算由基准守护，超了再换编码层（决策可逆）。
- **Alternatives considered**: 一步到位 MessagePack（过早优化、调试成本）。

## R10 撤销/重做：核心侧逆命令日志

- **Decision**: 每条 EditCommand 生成逆命令入栈（`edit/`），`ReplaceFromOutline`
  存全文对作为单一边界；栈不因保存截断。
- **Rationale**: O(每次编辑) 内存、天然精确回退（spec FR-012、Edge case"撤销跨
  生成边界"）；快照方案在 5,000 任务规模浪费内存。
- **Alternatives considered**: 全量模型快照（内存）；文本层 diff 撤销（与视图编辑
  命令不同构，边界易错）。

## R11 日期与工作日语义

- **Decision**: 天粒度本地日历日期；工作日 = 周一至周五固定，v1 无节假日日历；
  推导按拓扑序单遍（V-CYCLE 通过后），规则见
  [contracts/outline-grammar.md](./contracts/outline-grammar.md) §时间推导。
- **Rationale**: 与 spec Assumptions 一致；确定性简单，留节假日日历为后续特性。
- **Alternatives considered**: 时区/时刻支持（超范围）；可配置工作日（v1 不做，
  文法预留 `%` 指令空间）。

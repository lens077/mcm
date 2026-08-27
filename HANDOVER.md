# MCM 交接文档

面向接手这个项目的开发者。读完这份文档你应该能：跑起来、看懂架构为什么这样设计、
知道哪些地方**尚未验证**、以及从哪里继续。

**当前状态**：63/63 计划任务已实现，10 项自动化质量门全绿。
**尚未完成**：两种导出格式的真实互操作性、Windows 平台的任何实机验证。详见 §6。

---

## 1. 这是什么

面向开发者与产品经理的**桌面端项目规划工具**：

```
结构化大纲文本  →  确定性解析  →  统一模型  →  校验  →  四种联动视图
                                        ↓
                          XMind (.xmind) / Visio (.vsdx) 可继续编辑的导出
```

核心承诺（来自项目宪法）：跨平台等价、性能可量化、模型先行、人类可编辑的原生格式、
**导出必须可再编辑**（不是位图）。

---

## 2. 五分钟跑起来

```bash
# 前置：Rust stable、Node 20+、pnpm 9+
# macOS 还需 xcode-select --install；Windows 需 VS Build Tools + WebView2

pnpm install
pnpm tauri dev          # 开发运行
```

试一下：在左侧大纲框粘贴下面这段，点「生成规划」。

```text
%mcm 1
%title 移动端改版
%start 2026-09-01

- 需求阶段 #t1 @王芳
  - [x] 用户访谈 #t2 [2026-09-01..2026-09-05]
  - 竞品分析 #t3 [3d] <-t2
- 设计阶段 #t4
  - 交互稿 #t5 [5d] <-t3
! 需求冻结 #m1 [2026-09-30] <-t5
```

想看校验能力：把 `<-t2` 改成 `<-t5` 造一个环，问题面板会给出完整环路径。

---

## 3. 架构：三个必须理解的决策

### 3.1 模型是唯一事实来源，渲染是它的纯函数

```
crates/mcm-core/         ← 一切"规划是什么"的判断都在这里
  model/                 实体：Plan / Task / Dependency / Milestone
  outline/               解析 ↔ 序列化（同一套文法）
  validate/              13 条规则 + 时间推导
  layout/                四种布局算法
  scene/                 SceneGraph 投影
  edit/                  封闭命令集 + 撤销日志
  session.rs             会话容器

src/                     ← 前端只做渲染与手势，不持有可变模型状态
src-tauri/               ← 薄壳：命令转发、文件对话框、监测
```

**关键约束**：前端**没有**自己的模型副本。任何编辑都经 `edit_apply` 命令进入
Rust 核心 → 重校验 → 返回 `ApplyResult`（含 `scene_stale`）→ 前端重取场景。
这样"视图显示的东西"不可能偏离"模型里的事实"。

改前端时若发现"想在 TS 里算一下"，先停一下——那大概率该在 core 里算。

### 3.2 原生文件 = 大纲文本本身

`.mcm` 文件的正文就是你在编辑器里写的那套语法，加一行 `%mcm 1` 版本头。
没有第二套格式，没有 JSON 中间层。所以：

- 用户手写的和应用存的是同一种东西
- 文件天然可 diff、可进 Git
- 保存时规范化（固定注解顺序、恒写 ID），行级 diff 稳定

文法契约在 `specs/001-project-planning-tool/contracts/outline-grammar.md`，
**改文法前先读它**，往返律（`parse(serialize(m)) == m`）有属性测试守着。

### 3.3 导出是自研的，因为 Rust 生态里没有

调研确认（`research.md` R5/R6）：Rust 没有 `.xmind` 或 `.vsdx` 的**生成**库
（`libvisio-rs` 只能读）。所以两个导出器都是按格式规范手写的：

- **XMind**：ZIP（STORE 不压缩）+ `content.json`/`metadata.json`/`manifest.json`。
  依赖映射为**真实的 relationship 连线**，不是降级成文本。
- **Visio**：OPC 包 + XML。任务是**无 master 矩形**（Visio 自己就这么存），
  连接器实例化一个 Dynamic connector master。粘连用**双保险**：
  `_WALKGLUE`/`_XFTRIGGER` 公式 **加** `<Connects>` 行——这是 Visio 的字节级做法。

两个导出器都在**落盘前自检**：打包 → 重新解包 → 校验结构与引用闭合 → 才写文件。
失败就整个导出失败，绝不落半成品。

---

## 4. 测试策略：为什么是这样分层的

```bash
cargo test --workspace     # 409 个
pnpm test                  # 76 个
cargo bench -p mcm-core    # 7 项性能预算（超预算即 panic）
cargo run --release -p mcm-app -- --selftest    # 9 个端到端场景
```

四类测试各有分工：

| 类型 | 位置 | 守护什么 |
|------|------|---------|
| 单元测试 | 各模块内 `mod tests` | 单个函数的行为 |
| **属性测试** | `outline_roundtrip.rs`、`edit_undo.rs` | 往返律、apply∘undo 恒等 |
| **契约测试** | `tests/*_contract.rs` | 导出物结构、schema、降级完备 |
| 场景 harness | `src-tauri/src/selftest.rs` | 端到端串起来能用 |

**属性测试值得特别说明**——本次开发中它抓到了四个我没预料到的真实缺陷：

1. 含空格的负责人破坏往返律 → 加 `@"..."` 引号语法
2. 标题中连续空格被压缩（`一  a` → `一 a`）→ 整体加引号保护
3. 删除任务后撤销，兄弟顺序错乱 → `RestoreTasks` 快照全量 order
4. 5000 任务校验 257ms 超预算 → 引入 `PlanIndex` 消除 O(n²)，降到 19ms

每个都补了回归测试。**加新功能时请保持这个习惯**：先让属性测试跑起来，它比你想得到的边界多。

### 双平台冒烟的特殊安排

`tauri-driver` 不支持 macOS。所以同一份场景清单跑两条路：

- **Windows**：`pnpm e2e:win`（WebDriver，走真实 UI）
- **macOS**：`pnpm smoke:mac`（`--selftest`，走真实核心）

两边覆盖同一批场景 ID（S1/S3/S5/S6/S7/S8/S9/S11），CI 各跑各的。

---

## 5. 常用命令

```bash
# 开发
pnpm tauri dev
pnpm tauri build              # 出安装包
pnpm build:mac-universal      # macOS 双架构

# 质量门（CI 会全跑一遍）
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo bench -p mcm-core
pnpm lint && pnpm build && pnpm test
pnpm smoke:mac                        # 或 pnpm e2e:win
node scripts/measure-startup.mjs      # 冷启动 P95 ≤ 2s
node scripts/check-bundle-size.mjs    # 安装包 ≤ 25MB

# 造性能夹具
cargo run -p mcm-core --bin gen_fixture -- 1000 fixtures/perf/plan-1000.mcm
```

---

## 6. ⚠️ 尚未验证的部分（接手后优先处理）

诚实地说明边界——以下内容**在本次开发中没有条件验证**：

### 6.1 两种导出的真实互操作性（最高优先级）

自动化测试验证了：包结构、JSON schema、XML 良构、引用闭合、ID 唯一、
降级完备，VSDX 还通过 `libvisio-rs` 做了第三方读回。

**但「在 XMind / Visio 里打开并继续编辑」没有人验证过。** 尤其是 Visio 的
「拖动形状连线跟随」——粘连实现是按调研的实证结构写的，逻辑上正确，
但没有 Visio 环境实测。

→ 请按 `specs/001-project-planning-tool/release-checklist.md` 第 1、2 节逐项走。
发现问题时：**先补一个能复现的测试，再修**。

### 6.2 Windows 平台

本次开发全程在 macOS。以下从未在 Windows 上执行过：

- `pnpm e2e:win`（WebDriver 套件已写好但从未运行，首次跑很可能需要调整选择器）
- `pnpm tauri build` 的 Windows 产物
- CI 矩阵的 windows-latest 分支

### 6.3 CI

`.github/workflows/ci.yml` 已配置完整，但**从未在 GitHub Actions 上实际运行过**
（本地仓库刚建）。首次 push 后请关注运行结果。

### 6.4 项目名

宪法中留有 `TODO(PROJECT_NAME)`——"MCM" 取自目录名，非正式定名。
确定后改 `.specify/memory/constitution.md` 并做一次 PATCH 版本修订。

---

## 7. 从哪里继续

规格里明确**划在范围外**的（想做需要新开特性）：

- **自然语言 AI 生成规划**——当初明确选择了「结构化大纲 + 确定性解析」路线，
  理由是宪法要求核心功能完全离线。若要加，应作为独立特性接入同一模型与校验管线。
- 反向导入（`.xmind` / `.vsdx` → 本工具）
- 实时协同编辑、云同步
- 工时/资源/成本核算
- 界面国际化（首版仅简体中文）

已知的技术债与增强点：

- Visio 导出的任务元数据目前塞在形状文本行里；契约中提到 **Shape Data 属性区**
  是更专业的做法，留作后续增强（需先验证不触发修复对话框）
- 时间线视图目前不支持节假日日历，工作日固定为周一至周五
- 场景图走 JSON 传输；契约预留了「超预算则改二进制编码」的逃生门（research R9），
  当前 1000 任务下余量充足，暂未启用

---

## 8. 文档地图

```
.specify/memory/constitution.md      项目宪法 — 七条原则，优先于一切实践
specs/001-project-planning-tool/
  spec.md                            需求：6 个用户故事、22 条 FR、9 条 SC
  plan.md                            技术方案 + 宪法符合性检查
  research.md                        11 项技术决策及其理由（含被否决的方案）
  research-xmind.md                  XMind 格式调研（官方生成器源码级）
  research-vsdx.md                   VSDX 格式调研（MS-VSDX + 真实文件解剖）
  data-model.md                      实体、13 条校验规则、编辑命令集
  contracts/                         5 份契约 ← 改对应模块前必读
  quickstart.md                      环境、命令、11 个验证场景
  tasks.md                           63 个任务的实现记录
  release-checklist.md               发布前人工验收清单
```

**遇到"为什么这么设计"的疑问，答案大概率在 `research.md` 里**——包括那些
被否决的方案（Electron、ELK 布局、模板改写式导出）及其否决理由。

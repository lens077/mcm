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
# 前置：Rust stable、Node 24+、pnpm 11+
# macOS 还需 xcode-select --install；Windows 需 VS Build Tools + WebView2

make install
make dev
```

`make` 查看全部命令。没装 make（Windows 默认没有）时，每条规则的实现就是等价的
pnpm/cargo 命令，直接照抄即可。

**Makefile 不只是别名表**，它编码了两条真实依赖：

- `dist/` 是所有 cargo 目标的前置条件 —— Tauri 的 `generate_context!` 在编译期
  嵌入前端产物，干净检出时先跑 cargo 必然失败（CI 曾栽在这里，见 §6.3）。
  `make test-rs` 会自动先构建前端。
- `node_modules` 依赖 lockfile，变了自动重装。

关键目标：

| 命令 | 用途 |
|---|---|
| `make dev` | 启动应用 |
| `make gate` | 提交前的完整质量门（等价于 CI 的质量步骤） |
| `make ci` | 完整复现 CI（再加打包、体积、冷启动预算） |
| `make verify-clean-checkout` | 删掉 `dist/` 再跑门禁，复现 CI 的干净检出 |
| `make bundle` | 打包安装包；macOS 权限被拒时自动降级，见下 |
| `make release BUMP=minor` | 手动发版（默认由 push 自动 patch 递增） |
| `make ci-fail` | 最近一次 CI 失败的错误行，排查第一步 |
| `make verify-checksums TAG=v0.1.1` | 下载该版本附件并校验 SHA256 |

`make` 列出全部目标，发布与 CI 观测类命令都在其中。

### macOS 打包：权限被拒时自动降级

Tauri 打 `.dmg` 时调用 `bundle_dmg.sh`，后者用 `osascript` 驱动 Finder 美化磁盘
映像窗口（背景图、图标位置），需要「自动化」权限。拒绝授权时脚本以退出码 64
失败，**整个打包随之失败**——尽管 `.app` 早已产出，且 `.dmg` 本身并不依赖那段美化。

`scripts/bundle.mjs` 处理了这件事：先走正常路径，仅当确认踩到这个特定失败时，
才带 `CI=true` 重跑（Tauri 据此给脚本传 `--skip-jenkins` 跳过美化）。

- 授权可用 → 完整外观的 `.dmg`
- 授权被拒 → 自动降级，`.dmg` 仍可挂载安装，仅缺自定义背景与图标布局
- **其他构建错误照常失败**，绝不掩盖（有回归测试覆盖）

判据是 AppleScript 的 `-1743`（`errAEEventNotPermitted`）。注意 macOS 的授权按
**发起进程**归属：在 IDE 内置终端里跑就得授权那个 IDE，独立终端里能过不代表
IDE 里也能过。

> 顺带修掉一个潜在缺陷：失败那次会遗留读写中间映像 `rw.<pid>.*.dmg`，
> 而体积检查器只认 `.dmg` 后缀，把这个 36MB 的工作文件当成了交付物、误报超标。
> 现已在两处分别处理——打包前清理、检查时忽略。

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
- **Visio**：OPC 包 + XML。**所有形状都不用 master**，全部内联 Geometry；
  定位单元格一律写纯数值（不写公式）；粘连靠 `<Connects>` 行承载。
  这套写法是实测倒逼出来的，见下方"踩过的坑"。

两个导出器都在**落盘前自检**：打包 → 重新解包 → 校验结构与引用闭合 → 才写文件。
失败就整个导出失败，绝不落半成品。

---

## 4. 测试策略：为什么是这样分层的

```bash
make gate                  # 一条命令跑完下面全部（推荐）

# 或者逐项来。注意顺序：
pnpm build                 # ⚠️ 必须先跑：Tauri 的 generate_context! 宏在编译期
                           #    嵌入前端产物，dist/ 不存在会直接编译失败
cargo test --workspace     # 415 个
pnpm test                  # 76 个
cargo bench -p mcm-core    # 7 项性能预算（超预算即 panic）
pnpm smoke                 # 9 个端到端场景（双平台通用）
actionlint                 # 校验 .github/workflows/*.yml（brew install actionlint）
```

**踩坑提醒**：`dist/` 被 gitignore，本地通常已存在所以感觉不到；干净检出（CI）
若在 `pnpm build` 之前执行任何 cargo 命令，都会以
`The frontendDist configuration is set to "../dist" but this path doesn't exist`
失败。CI 的步骤顺序已固定，本地复现用 `rm -rf dist` 即可。

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

### 踩过的坑：VSDX 连接线完全不渲染

值得单独记一笔，因为它暴露了一类测试盲区。

初版严格照着调研的"最佳实践"实现：连接器实例化 Dynamic connector master，
写入 `_WALKGLUE`/`_XFTRIGGER`/`GUARD` 公式。契约测试**断言这些公式必须存在**，
全绿。但导出物在真实查看器里打开——**所有连接线消失**，只剩「依赖」「关联」文字。

两个原因：

1. `Master="2"`：第三方渲染器解析不了 master 引用，直接丢弃形状几何。
   矩形一直正常，正是因为它们无 master。**这是决定性原因。**
2. `_WALKGLUE`/`GUARD` 是 Visio 内部函数，第三方求值得 0 → `Width=0` → 线塌缩成点。

教训：**「结构合法」不等于「能被画出来」**。所有结构断言都通过了，因为它们检查的是
XML 形状，而不是可渲染性。现在补了一组渲染回归断言（无 master、无公式、
线长 > 0.1in、包围盒非退化、形状不重叠），并把「向已被证明能渲染的矩形靠拢」
作为连接线的设计原则。

复现验证方式（macOS 无 Visio 时）：

```bash
npm install playwright && npx playwright install chromium
# 用 Playwright 驱动 https://products.groupdocs.app/viewer/vsdx 上传并截图
```

### 双平台冒烟的安排

`pnpm smoke`（`--selftest`）是纯 Rust、与平台无关，因此 **CI 在 macOS 与
Windows 上都跑它**——同一批场景 ID（S1/S3/S5/S6/S7/S8/S9/S11）构成真正的
双平台门槛。

`tauri-driver` 不支持 macOS，所以 WebDriver UI 套件只能在 Windows 上跑。它
**尚未在真实 Windows 机器上验证过**，放进主 CI 只会让分支长期飘红，因此挪到了
独立的手动工作流 `.github/workflows/e2e-windows.yml`（`workflow_dispatch`
触发，wdio 依赖按需安装，不污染主依赖树）。

跑绿之后再把它提升进 `ci.yml`，步骤见该工作流顶部注释。

---

## 4.5 发布

**打标签才发版。** 推送 `vX.Y.Z` 形式的标签，`release.yml` 才构建 macOS 与
Windows 安装包。普通的代码推送只跑 CI，不产生版本。

```bash
make release              # patch：0.1.3 -> 0.1.4
make release BUMP=minor   # 0.1.3 -> 0.2.0
make release BUMP=major   # 0.1.3 -> 1.0.0
make release-watch        # 跟踪构建
```

`make release` 一步完成：递增版本 → 提交 → 打标签 → 推送 main 与标签。
前置检查从严（工作区必须干净、必须与 `origin/main` 一致）——发版是对外动作，
出错代价比多等一次高。

**标签为什么带 `v` 前缀**：git 标签用 `vX.Y.Z` 是生态惯例（Rust、Go 及绝大多数
项目），GitHub Release 页面也默认这么显示；而 Cargo/npm 的 `version` 字段本身
不带 `v`。两者分工清晰，本仓库已有的 `v0.1.0`~`v0.1.3` 就是这个形式。

**标签必须与代码内版本一致**，否则 `verify` 任务直接失败。原因很实际：安装包
文件名内嵌的是代码里声明的版本，不一致会导致产物名与 Release 标签对不上。
所以务必用 `make release` 而不是手工 `git tag`。

版本在两处声明，由 `scripts/bump-version.mjs` 同步（两者不一致时直接报错）：
`Cargo.toml` 的 `[workspace.package]` 与 `src-tauri/tauri.conf.json`。
`Cargo.lock` 交给 `cargo update -w` 刷新。

产出：`.dmg`（macOS）、`.msi` 与 NSIS `.exe`（Windows），附 `SHA256SUMS`。

两个实现上的坑，改动时注意：

- **安装包按后缀收集，不要写死路径**。`bundle.targets` 是 `"all"`，Windows 会
  同时产出 `.msi` 与 NSIS `.exe`——首次发布就证实了这点，写死单一 glob 会静默
  漏掉一个。收集时还要排除 `rw.<pid>.*.dmg` 中间映像。
- **校验和清单不能带 `./` 前缀**，否则下载后在别的目录 `sha256sum -c` 会找不到
  文件。另注意 `find -printf` 是 GNU 扩展，macOS 上无法本地验证，别用。

### CI 的路径过滤

`ci.yml` 与 `release.yml` 共用同一份代码白名单：只有 `src/`、`src-tauri/`、
`crates/`、依赖与工具链声明、以及直接影响构建产出的配置才触发。改文档、
`Makefile`、`scripts/`、`site/` 一律不跑重活（完整门禁约 20 分钟机器时间）。

**两处白名单要一起改。** 非代码改动想手动验证时用 `workflow_dispatch`。

## 4.6 宣传站点

`site/` 是 Astro 7 静态站，线上地址 https://mcm.apikv.com ，部署在 node1，
经 Pangolin/Traefik 暴露。开发与部署见 [site/README.md](./site/README.md)。

两点容易踩的：

- **`site/` 必须与根工作区解耦**（靠它自己的 `pnpm-workspace.yaml`）。否则
  Astro 依赖进根 `pnpm-lock.yaml`，而该文件在 release 触发白名单内——
  改落地页就会误发版本。曾试过 `.npmrc` 的 `ignore-workspace`，实测该键
  不被 pnpm 支持。
- **Pangolin 新建资源默认 `sso: true`**，会把访客重定向到登录页。公开站点
  必须显式关掉。

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

**已验证**：VSDX 在 GroupDocs（Aspose 内核）在线查看器中渲染正确——4 个任务矩形、
3 条依赖箭头、1 条里程碑关联箭头、菱形里程碑、完成态填充，全部到位，无重叠。
XMind 已由项目所有者在真实 XMind 中确认可用。

**仍未验证**：
- **Visio 桌面版**：能否无修复提示打开、**拖动形状时连线是否跟随**。
  第三方渲染器只证明了几何正确，粘连行为要 Visio 本体才能确认。
- 在 XMind / Visio 中**编辑后再保存**是否无损。

→ 请按 `specs/001-project-planning-tool/release-checklist.md` 第 1、2 节逐项走。
发现问题时：**先补一个能复现的测试，再修**。

### 6.2 Windows 平台

本次开发全程在 macOS。以下从未在 Windows 上执行过：

- WebDriver 套件（`.github/workflows/e2e-windows.yml`，手动触发）：已写好但
  从未运行，wdio 依赖也未进主依赖树；首次跑很可能需要调整选择器与超时
- `pnpm tauri build` 的 Windows 产物与 `.msi` 体积（图标已补齐，但 `.msi`
  从未实际产出过）

### 6.3 CI

首次运行暴露了三个真实缺陷，均已修复：

1. **步骤顺序**：cargo 在 `pnpm build` 之前跑，`dist/` 不存在导致
   `generate_context!` 编译失败。已把 `pnpm build` 提到最前。
2. **`pnpm e2e:win` 的 wdio 命令不存在**（依赖从未安装）。已移到手动触发的
   `e2e-windows.yml`。
3. **YAML 流式映射的坑**（`with: { ... }`）：
   - `{ key: ${{ matrix.os }} }` → `${{` 的花括号被当成嵌套映射，整个工作流
     文件解析失败，run 在 0 秒内挂掉
   - `{ components: clippy,rustfmt }` → 逗号把它切成两个键，`rustfmt`
     成了未定义输入，**rustfmt 组件其实从未被安装**（`cargo fmt` 能过是因为
     工具链自带）

   含表达式或逗号的 `with` 一律用块式写法。**改工作流后先跑 `actionlint`**——
   上面第 3 类问题它能直接指出来，不必靠 CI 往返。

   **Action 版本**：GitHub 已弃用 Node 20 运行时。当前全部固定在各自最新主版本，
   运行时均为 node24：

   | Action | 版本 | 备注 |
   |---|---|---|
   | `actions/checkout` | v7 | |
   | `pnpm/setup` | v2 | **同时装 pnpm 与 Node**，已取代 `actions/setup-node` |
   | `actions/upload-artifact` | v7 | |
   | `Swatinem/rust-cache` | v2 | v2 即最新主版本 |
   | `dtolnay/rust-toolchain` | @stable | composite，不涉及 Node 运行时 |

   升级时**逐个核对 `action.yml` 的 `runs.using`**，别按主版本号想当然：
   `actions/upload-artifact@v5` 就仍然是 node20，比它老的 v4 反而不是问题所在。
   一条可直接复用的核对命令见下。

   ```bash
   gh api repos/<owner>/<repo>/contents/action.yml?ref=<tag> -q '.content' \
     | base64 -d | grep -E "^\s*using:"
   ```

   pnpm 版本由 `package.json` 的 `packageManager` 字段单一决定，CI 不再重复声明。

   **已迁移到 `pnpm/setup`**（`pnpm/action-setup` 的继任者，面向 pnpm v11+）。
   它下载 pnpm 自包含二进制并顺带安装 Node，因此 `actions/setup-node` 已移除。
   两处有意偏离官方迁移指引：

   - `install: false`：指引建议让 action 顺带跑 `pnpm install`，但
     `--frozen-lockfile` 是合入门槛（lockfile 必须已提交且最新），这个保证
     应当在工作流里显式可见，而不是依赖 CI 环境变量的隐式默认。
   - 脚本一律走 `pnpm <script>` 而非直接 `node`，不依赖 PATH 细节。

   Node 版本以 `runtime: node@24` 写在工作流里。也可以改放进 package.json 的
   `devEngines.runtime` 由 action 自动读取，但那会给本地开发者引入额外约束，
   暂未采用。
4. **Windows 换行**：Git 在 Windows 检出时把 `.mcm` fixture 重写成 CRLF，
   golden 测试拿原始字节比对 LF 输出，断言失败。产品本身没问题
   （`str::lines` 已剥掉行尾 `\r`，词法层还再剥一次）。修法是
   `.gitattributes` 钉死 LF + 测试加载 fixture 时归一化换行。

   > 顺带一提：我一度想在 `parse()` 里加"防御性归一化"，属性测试立刻指出
   > 那会把孤立的 `\r` 也当成换行、改变损坏文档的行数语义。改动已回退——
   > **产品原本就是对的，别改对的东西**。

5. **Windows 缺 `icons/icon.ico`**：`tauri-build` 生成 Windows 资源文件时必需。
   原先只有 PNG 图标，macOS 能过、Windows 直接构建失败。现已补齐
   `icon.ico`（6 种尺寸）与 `icon.icns`，两者都登记在 `tauri.conf.json`
   的 `bundle.icon` 里。

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

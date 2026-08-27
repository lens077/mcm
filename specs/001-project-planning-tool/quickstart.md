# Quickstart: 项目规划桌面工具

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)
本文是环境搭建、运行与端到端验证指南；实现细节见 `tasks.md`（Phase 2 生成）与各契约。

## 环境准备

| 依赖 | 版本 | 说明 |
|------|------|------|
| Rust | stable（rustup 安装） | workspace 构建；`cargo` 随附 |
| Node.js + pnpm | Node 20+ / pnpm 9+ | 前端与 Tauri CLI |
| macOS | 10.15+，Xcode Command Line Tools | `xcode-select --install`；universal 构建需 `rustup target add aarch64-apple-darwin x86_64-apple-darwin` |
| Windows | 10+，VS Build Tools（C++ 桌面负载）+ WebView2 Runtime | Win11 自带 WebView2 |

```bash
pnpm install          # 前端依赖 + tauri CLI
cargo fetch           # 预拉取 Rust 依赖（锁定于 Cargo.lock）
```

## 运行与构建

```bash
pnpm tauri dev        # 开发运行（热载前端；Rust 变更自动重编）
pnpm tauri build      # 发行构建（当前平台安装包；预算 ≤ 25MB）
```

## 测试命令（全部为合入门槛，CI 于 macOS + Windows 矩阵执行）

```bash
cargo test --workspace        # mcm-core 单测/golden/属性测试 + mcm-export 契约测试
cargo bench -p mcm-core       # criterion 性能基准（预算断言见下）
pnpm test                     # Vitest 前端单元测试
pnpm e2e:win                  # WebdriverIO + tauri-driver 冒烟（Windows CI）
pnpm smoke:mac                # macOS 内置场景自检 harness（research.md R8）
```

## 端到端验证场景（对应用户故事与成功标准）

| # | 场景 | 步骤 | 预期 | 覆盖 |
|---|------|------|------|------|
| 1 | 生成与校验 | 新建 → 在大纲编辑器粘贴 [outline-grammar.md](./contracts/outline-grammar.md) 示例 → 生成 | 四视图呈现完整规划，问题面板为空 | US1, FR-001..005 |
| 2 | 校验定位 | 在示例中把 `<-t2` 改为 `<-t4` 制造环 | V-CYCLE 报完整环路径并定位；修复后问题消失 | US1-2, SC-007 |
| 3 | 视图联动 | WBS 选中任务 → 依次切换其余三视图 | 选中保持并自动定位；主题切换双向清晰 | US2, FR-006..009 |
| 4 | 规模性能 | 打开 `fixtures/perf/plan-1000.mcm` → 缩放/拖拽/编辑 | 无可感知卡顿；perf overlay 帧率 ≥ 60、编辑反馈 ≤ 100ms | SC-002 |
| 5 | 直接编辑 | 拖拽换父、画依赖线、改日期、连续撤销/重做 | 全视图同步、非法即标注、撤销链精确 | US3, FR-010..012 |
| 6 | 保存往返 | 保存 → 重开 → 对比 | 内容零丢失；文本编辑器可读；再保存字节稳定 | US4, SC-006 |
| 7 | 手工编辑与恢复 | 文本编辑器改文件（含一行故意写坏）→ 重新打开 | 合法修改生效；坏行隔离为 P-* 问题 + `[mcm:recovered]` 注释 | US4-2/3, FR-015 |
| 8 | 导出 XMind | 导出 `.xmind` → 跑校验器 → XMind 打开编辑保存 | 契约测试绿；XMind 编辑无报错；降级项全部出现在导出摘要 | US5, SC-004, SC-008 |
| 9 | 导出 Visio | 导出 `.vsdx` → 跑校验器 → Visio 打开：拖动形状 | 无修复警告；连线跟随形状；文本可编辑 | US6, SC-005 |
| 10 | 冷启动 | `scripts/measure-startup`（两平台各 20 次取 P95） | ≤ 2s | SC-003 |
| 11 | 关闭保护 | 有未保存更改时关窗 | 提示保存/放弃/取消 | FR-016 |

## 性能预算断言（cargo bench / CI 阻断）

| 基准 | 预算 |
|------|------|
| 解析 5,000 行大纲 + 全量校验 | ≤ 200ms |
| 1,000 任务单条编辑（含增量校验 + 场景投影） | 核心 ≤ 50ms |
| 1,000 任务 scene_get（每视图） | ≤ 50ms |
| 1,000 任务导出（每格式） | ≤ 3s |

## 发布前人工验收（双平台各一遍）

1. 场景 1–11 全过；
2. XMind（当前稳定版）与 Visio（2016+ 任一）真实打开导出件并编辑保存（SC-004/005 的
   最终人工确认，自动化部分由契约测试承担）；
3. 深浅主题截屏对比检查（宪法 III）。

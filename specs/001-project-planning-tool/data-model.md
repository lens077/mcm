# Data Model: 项目规划桌面工具

**Feature**: [spec.md](./spec.md) | **Date**: 2026-08-26
**落位**: `crates/mcm-core/src/model/`（实体）、`validate/`（规则）、`edit/`（命令）

模型是全系统唯一事实来源（宪法 IV）。所有视图、文件、导出物均为其确定性投影。

## 基础类型

| 类型 | 定义 | 约束 |
|------|------|------|
| `TaskId` / `MilestoneId` | 短标识，形如 `t7` / `m2`（前缀 + 十进制序号） | 文档内唯一；由解析器/编辑命令分配；改名不变；会话内单调不复用 |
| `Date` | 日历日期（ISO `YYYY-MM-DD`），天粒度，本地语义 | 无时区、无时刻（spec Assumptions） |
| `Duration` | 工作日天数 `Nd`（N ≥ 1 整数） | 周末为非工作日；v1 无节假日日历 |
| `Schedule` | `None` \| `Explicit{start, end}` \| `Duration{days}` | Explicit 要求 start ≤ end（V-RANGE） |

## 实体

### Plan（规划根）

| 字段 | 类型 | 说明 |
|------|------|------|
| `title` | string | 非空；缺省 "未命名规划" |
| `description` | string? | 可选描述 |
| `format_version` | int | 当前 = 1（对应文件头 `%mcm 1`） |
| `project_start` | Date? | 推导锚点；缺省取全部显式日期最早者，再缺省为打开当日 |
| `tasks` | Task 森林 | 有序（同级顺序 = 文档顺序） |
| `dependencies` | Dependency[] | 任务间约束 |
| `milestones` | Milestone[] | 里程碑集合 |

### Task（任务）

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | TaskId | 稳定标识 |
| `title` | string | 1..500 字符，非空（V-TITLE） |
| `parent` | TaskId? | 空 = 顶层；父子构成 WBS 层级 |
| `order` | int | 同级排序（连续、由结构操作维护） |
| `schedule` | Schedule | 见基础类型 |
| `assignee` | string? | 负责人（自由文本，v1 无人员库） |
| `notes` | string? | 多行备注 |
| `done` | bool | 完成标记 |

**派生字段**（计算所得，不持久化）：`effective_start` / `effective_end`
（显式日期 > 由 Duration 沿依赖链推导 > 由子任务汇卷取包络）、`depth`、`has_issues`。
推导规则详见 [contracts/outline-grammar.md](./contracts/outline-grammar.md) §时间推导。

### Dependency（依赖）

| 字段 | 类型 | 说明 |
|------|------|------|
| `predecessor` | TaskId | 前置任务 |
| `successor` | TaskId | 后继任务 |
| `kind` | enum | v1 仅 `FinishToStart`（完成-开始） |

### Milestone（里程碑）

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | MilestoneId | 稳定标识 |
| `name` | string | 非空 |
| `date` | Date | 检查点日期 |
| `linked_tasks` | TaskId[] | 关联任务（可空） |

### ValidationIssue（校验问题）

| 字段 | 类型 | 说明 |
|------|------|------|
| `severity` | `Error` \| `Warning` | 见规则表 |
| `code` | string | 稳定编码 `V-*` / `P-*` |
| `target` | ElementRef | `Task(id)` \| `Dep(pred,succ)` \| `Milestone(id)` \| `Line(n)` \| `Plan` |
| `message` | string | 人类可读原因 |
| `fix_hint` | string | 修复指引（spec FR-004 必填） |
| `cycle_path` | TaskId[]? | 仅 V-CYCLE：完整环路径 |

### ExportReport（导出摘要）

| 字段 | 类型 | 说明 |
|------|------|------|
| `format` | `Xmind` \| `Vsdx` | 目标格式 |
| `output_path` | string | 产物路径 |
| `mapped` | Item[] | 成功映射项（种类 + 数量摘要） |
| `degraded` | Item[] | 降级项：元素、原表达、降级后表达（FR-021：逐项列明，0 静默丢失） |
| `warnings` | string[] | 其他提示（如存在校验错误仍导出） |

### SceneGraph（视图场景图，投影非实体）

`scene(plan, view_kind) → SceneGraph`，纯函数、确定性（宪法 IV）。

| 字段 | 说明 |
|------|------|
| `view` | `Wbs` \| `DepGraph` \| `Timeline` \| `Milestones` |
| `nodes[]` | `{ref, x, y, w, h, style_role, text_runs, badges}`——badge 含问题标记、里程碑旗标、完成态 |
| `edges[]` | `{from, to, points[], style_role}`（层级线 / 依赖线 / 里程碑关联线） |
| `bounds` | 内容包围盒（供视口适配） |

坐标为与主题无关的逻辑单位；颜色/字体由前端 design tokens 按 `style_role` 解析（宪法 III）。

## 校验规则（`validate/`，全部确定性）

| 编码 | 级别 | 规则 | 定位 | 修复指引示例 |
|------|------|------|------|-------------|
| V-REF | Error | 依赖/里程碑引用的 TaskId 必须存在 | Dep/Milestone | "删除该引用或改为现有任务 ID" |
| V-DUP | Error | 文档内 ID 不得重复 | Line | "为其中一个元素更换 `#id`" |
| V-SELF | Error | 任务不得依赖自身 | Dep | "删除自依赖" |
| V-CYCLE | Error | 依赖图必须无环（含跨层级传递）；报告完整环路径 | Dep + cycle_path | "断开环中任一依赖，建议 …" |
| V-HIER | Error | 祖先与后代之间不得建立依赖 | Dep | "依赖应建立在同层可比任务之间" |
| V-RANGE | Error | 显式日期 start ≤ end | Task | "交换或修正日期" |
| V-PARENT | Error | 子任务有效日期必须落在父任务显式日期范围内 | Task | "扩大父任务范围或调整子任务" |
| V-ORDER | Error | 后继 effective_start ≥ 前置 effective_end | Dep | "推迟后继开始日期或缩短前置" |
| V-MSTONE | Error | 里程碑日期 ≥ 所有关联任务 effective_end | Milestone | "推迟里程碑或提前关联任务" |
| V-TITLE | Error | 任务/里程碑名称非空 | Task/Milestone | "补写名称" |
| W-NODATE | Warning | 出现在时间线视图的任务缺少任何日期信息 | Task | "补充日期或工期" |
| W-ORPHAN | Warning | 依赖图中存在与主体完全不连通的孤立簇 | Task | "确认是否遗漏依赖" |
| P-0xx | Error | 解析错误族（语法/缩进/日期格式/引用格式），定位行列 | Line | 见 outline-grammar.md §错误码 |

执行时机：生成后、每条 EditCommand 应用后（增量重算受影响子集，预算 ≤ 100ms）。
问题集整体替换、非阻断标注（spec FR-011）；存在 Error 时导出前必须显式警示。

## EditCommand（封闭编辑命令集，`edit/`）

所有变更唯一入口；每条命令产生逆命令入撤销栈（spec FR-012）。

| 命令 | 参数 | 逆命令 |
|------|------|--------|
| `AddTask` | parent?, index, title | DeleteTask |
| `RenameTask` | id, title | RenameTask(旧值) |
| `DeleteTask` | id（级联删除子树 + 相关依赖/里程碑引用，全部记录入日志） | 复合恢复 |
| `MoveTask` | id, new_parent?, index | MoveTask(原位置) |
| `SetSchedule` | id, schedule | SetSchedule(旧值) |
| `SetAssignee` / `SetNotes` / `SetDone` | id, value | Set*(旧值) |
| `AddDependency` | pred, succ | RemoveDependency |
| `RemoveDependency` | pred, succ | AddDependency |
| `AddMilestone` / `UpdateMilestone` / `RemoveMilestone` | … | 对应逆操作 |
| `SetPlanMeta` | title/description/project_start | SetPlanMeta(旧值) |
| `ReplaceFromOutline` | 全文本（大纲编辑器提交 / 重新生成） | ReplaceFromOutline(旧文本)——单一撤销边界（spec Edge Case） |

## 状态与生命周期

**文档会话**：`Empty → Loaded{clean} → Loaded{dirty} →(save: 临时文件+原子改名)→
Loaded{clean}`；关闭时 dirty 必须提示（FR-016）。打开损坏文件 →
`Loaded{recovered, issues}`：可解析部分入模型，不可解析行以 `P-*` 问题呈现（FR-015）。

**校验生命周期**：`模型变更 → 增量重校验 → 问题集替换 → 场景图重投影 → 前端重绘`。
链路同步完成，预算 100ms（1,000 任务内）。

**撤销栈**：会话内不限深度；`undo[] / redo[]`；新命令清空 redo；保存不截断栈。

## 实体关系图（文字版）

```text
Plan 1──* Task（parent 自引用构成森林）
Plan 1──* Dependency（predecessor/successor → Task，FS）
Plan 1──* Milestone（linked_tasks *──* Task）
Plan 1──* ValidationIssue（target → Task/Dep/Milestone/Line/Plan，派生非持久）
导出：Plan ──ExportReport（每次导出一份）
投影：Plan ──scene()──> SceneGraph × 4 视图
```

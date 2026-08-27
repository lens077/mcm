# Contract: 导出 XMind（.xmind）

**目标**: 生成 XMind 2020–2026 可打开、可继续编辑、可再保存的脑图文件（spec US5、
FR-018/FR-020/FR-021；宪法 VI）。
**依据**: [research-xmind.md](../research-xmind.md)（官方 xmind-generator 序列化器为
基准模板）。**落位**: `crates/mcm-export/src/xmind/`

## 包结构（ZIP，条目 STORE 不压缩，UTF-8，小写条目名）

| 条目 | 内容 |
|------|------|
| `content.json` | 顶层**数组**，单 sheet（见映射） |
| `metadata.json` | `{"creator":{"name":"MCM"},"dataStructureVersion":"2"}` |
| `manifest.json` | `{"file-entries":{"content.json":{},"metadata.json":{}}}` |

不生成 `Thumbnails/`、`content.xml`（XMind 2020+ 不需要；≤XMind 8 不在兼容范围，
见 spec Assumptions）。

## 模型 → XMind 映射

| 模型元素 | XMind 表示 | 保真级别 |
|----------|-----------|---------|
| Plan.title | sheet.title 与 rootTopic.title | 映射 |
| Task 层级（文档序） | rootTopic 下递归 `children.attached[]`，topic.title = 任务标题 | 映射 |
| Task.id | topic id 采用确定性派生 UUID（v5 风格：命名空间 + TaskId），并在 topic 标签中保留 `#t<n>` | 映射 |
| Task.done | marker `task-done` | 映射 |
| Task.notes | `notes.plain.content`（尾随 `\n`） | 映射 |
| Task.assignee | label `@<负责人>` | **降级**（XMind 无负责人字段 → 标签） |
| Task 日期/工期 | label `2026-09-01..2026-09-05` / `5d` | **降级**（XMind 无日期语义 → 标签文本） |
| Dependency | sheet 级 `relationships[]`：`end1Id`=前置 topic、`end2Id`=后继 topic、`title`="依赖" | 映射（真实连线，可编辑） |
| Milestone | rootTopic 下专设分支 "里程碑" 的子 topic，marker `flag-red`，label 为日期 | **降级**（脑图无里程碑概念 → 旗标节点） |
| Milestone.linked_tasks | relationship（`title`="关联"）连接里程碑 topic 与任务 topic | 映射 |
| 时间推导结果 | 不导出（XMind 端不可计算） | **降级**（摘要中说明） |

**ExportReport 义务**（FR-021）：上表标"降级"的每一类，逐元素列入 `degraded[]`，
说明原表达与降级后表达；无任何静默丢失。存在校验 Error 仍导出时写入 `warnings[]`。

## 生成规则（防"文件损坏"）

- 所有 id 文件内唯一；relationship 端点必须指向存在的 topic id。
- 每个 sheet/topic 必含 `id`、`class`、`title`（空标题写 `""` 不省略——但模型侧
  V-TITLE 已保证非空）。
- 同组 marker 每 topic 至多一枚。
- 严格 UTF-8 无 BOM；`manifest.json` 列出全部条目。
- 输出前内部自检：重新解包 + JSON schema 校验 + 引用闭合检查，失败即导出失败
  （绝不落半成品文件）。

## 契约测试（`mcm-export` 集成测试，合入门槛）

1. **结构断言**：导出 fixtures（含中文/emoji/极长标题/1,000 任务），解包后断言：
   三条目在场且仅此三条目、条目 STORE、content.json 顶层为数组、id 全唯一、
   relationship 端点闭合、任务树与模型同构（顺序含）。
2. **Schema 校验**：content.json 通过仓库内维护的 JSON Schema
   （`fixtures/xmind/content.schema.json`，按 research-xmind.md §2 制定）。
3. **降级完备**：构造含日期/负责人/里程碑的模型 → ExportReport.degraded 逐项断言；
   报告项数 = 模型中不可映射项数（0 静默丢失，对应 SC-008）。
4. **第三方互读**：CI 中用独立读取实现（移植 xmindparser 读取逻辑的最小校验器）
   重新读出导出文件并比对树结构。
5. **人工验收清单**（发布门槛，双平台）：XMind 打开 → 编辑节点文本 → 拖动结构 →
   保存无报错 → 重开正常（对应 SC-004）。

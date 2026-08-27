# Contract: 导出 Visio（.vsdx）

**目标**: 生成 Visio 2016+ 无修复对话框打开、形状与连线全部可编辑、且连线随形状
移动保持粘连的依赖网络图（spec US6、FR-019/020/021；宪法 VI）。
**依据**: [research-vsdx.md](../research-vsdx.md)（MS-VSDX 规范 + 真实 Visio 文件
解剖）。**落位**: `crates/mcm-export/src/vsdx/`

## 结构决策

- **矩形任务形状不用 master**（Visio 自身即如此保存：内联 Geometry
  RelMoveTo + 4×RelLineTo，完全可编辑）。
- **连接器实例化单一 "Dynamic connector" master**（仓库自维护
  `fixtures/vsdx/dynamic-connector-master.xml`，按 BaseID
  `{F7290A45-E3AD-11D2-AE4F-006008C9F5A9}`、`MatchByName='1'`、母形状
  `ObjType='2' GlueType='2' DynFeedback='2'` 制作），保证路由行为并与用户后画的
  连接器合并。
- **粘连双保险**：既写 `_WALKGLUE`/`_XFTRIGGER`/`GUARD` 公式，又写 `<Connects>`
  行——这是 Visio 自身的字节级做法；对第三方文件 Visio 也会按 Connect 元素在打开
  时重建粘连公式。

## OPC 包结构（ZIP，deflate，无目录条目，无 BOM，`[Content_Types].xml` 首条目）

| Part | 必需 | 要点 |
|------|------|------|
| `[Content_Types].xml` | ✅ | `Default`: rels/xml；`Override` 逐 part 用准确的 `application/vnd.ms-visio.*+xml`（清单见 research-vsdx §1，逐字节采用） |
| `_rels/.rels` | ✅ | → `visio/document.xml`，type `…/visio/2010/relationships/document` |
| `visio/document.xml` + `visio/_rels/document.xml.rels` | ✅ | 根 `VisioDocument`；含 `StyleSheet ID='0'` 链或形状不引用样式；rels → pages + masters |
| `visio/pages/pages.xml` + `visio/pages/_rels/pages.xml.rels` | ✅ | `Page ID='0'`：`PageSheet`（`PageWidth`/`PageHeight` 英寸）+ `<Rel r:id>` 与 rels Id 一致 |
| `visio/pages/page1.xml` | ✅ | 根 `PageContents`：`<Shapes>` + `<Connects>` |
| `visio/masters/masters.xml` + `master1.xml` + rels | ✅（因连接器） | Dynamic connector master |
| `docProps/core.xml`/`app.xml`、`visio/windows.xml` | 可选 | 生成简洁存根（干净元数据） |

命名空间一律 `http://schemas.microsoft.com/office/visio/2012/main`（**不得**使用
MS-VSDX 文中的 `…/visio/2011/1/core`——那是 SharePoint web drawing 子集）；根元素带
`xml:space='preserve'`。

## 模型 → Visio 映射（单页：依赖网络）

坐标：取 `mcm-core` 依赖图布局场景（分层 DAG），比例 100 逻辑单位 = 1 英寸，
Y 轴翻转（Visio 原点在左下）；页尺寸 = 内容包围盒 + 1 英寸边距。

| 模型元素 | Visio 表示 | 保真级别 |
|----------|-----------|---------|
| Task | `<Shape ID Type='Shape'>` 矩形（PinX/PinY/Width/Height、LocPin 居中、内联 Geometry、`<Text>` 第 1 行 = 标题） | 映射 |
| Task.id / 日期 / 负责人 | `<Text>` 附加行：`#t3 · 2026-09-01..05 · @张三` | **降级**（文本行，Visio 可编辑；Shape Data 属性区留作后续增强） |
| Task.done | 文本行附 `✓`；形状 FillForegnd 用完成色 | **降级** |
| Dependency | Dynamic connector 实例：Begin/End 位于两形状边界（数值）+ `_WALKGLUE` 公式 + `BegTrigger/EndTrigger = _XFTRIGGER(Sheet.<ID>!EventXFMod)` + 两行 `<Connect>`（`FromPart 9/12` → `ToCell='PinX' ToPart='3'` 动态粘连） | 映射（真实粘连，拖动跟随） |
| Milestone | 菱形形状（RelMoveTo 0.5,0 → RelLineTo 1,0.5 / 0.5,1 / 0,0.5 / 闭合），文本 = 名称 + 日期 | 映射 |
| Milestone.linked_tasks | 同款连接器，`<Text>`="关联" | 映射 |
| 时间线几何（甘特条） | 不导出为时间轴图（日期在文本行中保留） | **降级**（摘要说明） |
| WBS 层级 | 不在依赖网络页表达（任务全集平铺分层） | **降级**（摘要说明：层级路径写入形状文本可选行） |

**ExportReport 义务**（FR-021）：降级四类逐元素列入 `degraded[]`；存在校验 Error
仍导出 → `warnings[]`。

## 生成规则（防修复对话框，逐条来自 research-vsdx §6）

1. Shape ID 页内唯一且与 `<Connects>`、`Sheet.<ID>!` 公式一致（重复 ID 会被
   Visio **静默丢形状**——契约测试重点）。
2. Geometry `Row IX` 自 1 连续；首行必为 (Rel)MoveTo。
3. 数值单位英寸；`LocPinX F='Width*0.5'` 等公式照抄参考模式。
4. 严格 UTF-8 无 BOM；`<?xml version="1.0" encoding="utf-8"?>` 声明；`<Text>` 以
   字面 `\n` 结尾。
5. ZIP：part 名以包根相对（无文件夹前缀）、无目录条目、无多余文件；`zip` crate
   默认 deflate。
6. rel 链闭合：package→document→pages→page / document→masters→master1；孤儿 part
   即失败。
7. 输出前内部自检：重解包 + XML 良构校验 + ID/rel/Connect 引用闭合检查，失败即
   导出失败，不落半成品。

## 契约测试（`mcm-export` 集成测试，合入门槛）

1. **结构断言**：导出 fixtures（含中文/emoji/极长标题/1,000 任务 + 密集依赖），
   解包后断言：part 清单与 Content_Types Override 一一对应、rel 链闭合、Shape ID
   唯一、每依赖恰两行 Connect 且端点存在、Geometry 行序合法、无 BOM/目录条目。
2. **Golden diff**：小型固定模型 → 导出与 `fixtures/vsdx/golden-small/` 逐 part
   规范化 XML 比对（波动字段白名单化）。
3. **独立读回**：用 `libvisio-rs`（读取库）解析导出文件成功且形状/连线计数吻合，
   作为第三方互读关卡。
4. **降级完备**：构造含全部降级类别的模型 → `degraded[]` 项数与内容逐项断言
   （对应 SC-008）。
5. **人工验收清单**（发布门槛）：Visio 2016+ 打开无修复提示 → 拖动任务形状、
   连线跟随 → 编辑文本 → 保存无警告 → 重开正常；将 Visio 再保存文件 unzip diff
   一次，学习其归一化以回填 golden（对应 SC-005）。

# Contract: 导出 Visio（.vsdx）

**目标**: 生成 Visio 2016+ 无修复对话框打开、形状与连线全部可编辑、且连线随形状
移动保持粘连的依赖网络图（spec US6、FR-019/020/021；宪法 VI）。
**依据**: [research-vsdx.md](../research-vsdx.md)（MS-VSDX 规范 + 真实 Visio 文件
解剖）。**落位**: `crates/mcm-export/src/vsdx/`

## 结构决策

> **2026-08-27 修订（实证驱动）**：初版按 research-vsdx §4 的"最佳实践"实现——
> 连接器实例化 Dynamic connector master，并写入 `_WALKGLUE`/`GUARD` 公式。
> 在 GroupDocs（Aspose 内核）在线查看器中实测发现：**所有连接线完全不渲染**，
> 只剩文字标签。逐项排除后确认两个原因，本节据此改写：
>
> 1. `Master="2"` —— 第三方渲染器解析不了 master 引用，直接丢弃形状几何。
>    矩形一直正常正是因为它们无 master。**这是决定性原因。**
> 2. `_WALKGLUE`/`GUARD` —— Visio 内部函数，第三方求值得 0，`Width` 归零。
>
> 调研本身给了依据：对第三方文件，Visio 会**按 `<Connects>` 元素重建粘连公式**，
> 因此纯数值 + Connects 已经足够。取舍：放弃 Visio 内的自动路由与 `MatchByName`
> 合并（锦上添花），换取所有工具都能正确渲染（核心承诺 FR-019/020）。

- **所有形状一律不用 master**（Visio 自身即以内联 Geometry 保存矩形：
  RelMoveTo + 4×RelLineTo，完全可编辑）。连接器采用同一套写法，因为它是
  已被实测证明能被第三方渲染器画出来的形态。
- **所有定位单元格写纯数值，不写公式**：`PinX`/`PinY`/`Width`/`Height`/
  `BeginX`/`BeginY`/`EndX`/`EndY` 均为可直接读取的数字。
- **粘连由 `<Connects>` 承载**：两行 `<Connect>`（`FromPart 9/12` →
  `ToCell='PinX' ToPart='3'`）加形状上的 `ObjType='2' GlueType='2'`，
  Visio 打开时据此建立动态粘连。
- **连接器包围盒不得退化**：水平或垂直线的短边补足到 `MIN_SPAN_IN`，
  避免渲染器按零面积包围盒裁剪掉描边。

## OPC 包结构（ZIP，deflate，无目录条目，无 BOM，`[Content_Types].xml` 首条目）

| Part | 必需 | 要点 |
|------|------|------|
| `[Content_Types].xml` | ✅ | `Default`: rels/xml；`Override` 逐 part 用准确的 `application/vnd.ms-visio.*+xml`（清单见 research-vsdx §1，逐字节采用） |
| `_rels/.rels` | ✅ | → `visio/document.xml`，type `…/visio/2010/relationships/document` |
| `visio/document.xml` + `visio/_rels/document.xml.rels` | ✅ | 根 `VisioDocument`；含 `StyleSheet ID='0'` 链或形状不引用样式；rels → pages + masters |
| `visio/pages/pages.xml` + `visio/pages/_rels/pages.xml.rels` | ✅ | `Page ID='0'`：`PageSheet`（`PageWidth`/`PageHeight` 英寸）+ `<Rel r:id>` 与 rels Id 一致 |
| `visio/pages/page1.xml` | ✅ | 根 `PageContents`：`<Shapes>` + `<Connects>` |
| `visio/masters/*` | ❌ 不生成 | 见 §结构决策：master 引用会让第三方渲染器丢弃几何 |
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
| Task.id / 日期 / 负责人 | `<Text>` 附加行：`#t3 · 2026-09-01..05 · @张三`（日期同月只写收尾日，同年省年份，跨年写全） | **降级**（文本行，Visio 可编辑；Shape Data 属性区留作后续增强） |
| Task.done | 文本行附 `✓`；形状 FillForegnd 用完成色 | **降级** |
| Dependency | 无 master 的 1-D 形状：Begin/End 落在两形状边界（纯数值）+ `ObjType='2' GlueType='2'` + 两行 `<Connect>`（`FromPart 9/12` → `ToCell='PinX' ToPart='3'` 动态粘连） | 映射（真实粘连，拖动跟随） |
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

## 渲染回归（新增，源自真实缺陷）

以下断言直接对应上面那次实测失败，必须长期保留：

- `page1.xml` 中不得出现 `_WALKGLUE`/`_XFTRIGGER`/`GUARD(`
- 定位单元格（Pin/Width/Height/Begin/End）不得带 `F="..."`
- 任何形状不得带 `Master=` 属性
- 每条连接线的 `|Begin→End|` 长度必须 > 0.1 英寸（零长即不可见）
- 连接线包围盒与端点一致，且两轴均非零
- 2-D 形状两两不重叠（里程碑曾压住首个任务）

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

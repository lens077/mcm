# 导入图表：截图与 archify HTML

把别处画好的架构图 / 流程图转成 MCM 大纲，之后可以继续编辑，也可以导出为
XMind、Visio。入口是工具栏的「导入图表」，实现位于 `crates/mcm-import`。

| 输入 | 方式 | 精度 |
|------|------|------|
| PNG / JPEG / WebP / BMP 截图 | 本地 OCR + 方框、分组框、箭头识别 | 依赖识别，需要人工核对 |
| archify 生成的 `.html` | 读取 `data-node-*` / `data-edge-*` / 分组框标注 | 精确，不经过 OCR |

其他 HTML 会被明确拒绝，并提示改用截图导入，不会只导入一半。

## 映射规则

| 图中元素 | 大纲中的表示 |
|----------|--------------|
| 方框 | 任务。第一行文字是标题，其余行是备注（`> `） |
| 包含其他方框的框（分组、虚线边界） | 父任务，标题取框内顶部的文字 |
| 箭头 | 依赖：`A → B` 写成 B 的 `<-A` |
| 没有箭头的连线 | 依赖，按从上到下、从左到右定方向，并在报告中提示 |
| 连线上的文字 | 注释 `# 连线说明：A → B：文字`。规划模型的依赖没有标签 |
| 会形成环的箭头 | 不导入，写成注释 `# 为避免循环依赖未导入的连线：…`。规划必须无环（V-CYCLE） |
| 不在任何方框内的文字（图例、说明） | 注释 `# 图中未归属的文字：…` |
| 看图器自带的浅灰色、无连线的面板（缩放栏等） | 不作为任务；其中的文字按上一行处理 |

所有取舍都会写进大纲注释，并在导入对话框里列出，不会静默丢弃（宪法 VI）。
导入结果只是大纲文本，载入后仍走正常的解析和校验流程（宪法 IV）。

导入不会改动当前规划。用户在对话框里确认后，结果会作为一个新的未保存规划载入，
载入前的未保存确认与「新建」相同。

## OCR 选型

要求：完全离线、Windows 和 macOS 行为一致、中英混排准确、速度快、安装包不超过
25 MB 的预算。

### 候选

| 方案 | 结论 |
|------|------|
| [ocrs](https://github.com/robertknight/ocrs) | 纯 Rust，但只支持拉丁字母，排除 |
| Tesseract | 中文小字识别差，还要引入 C++ 依赖，排除 |
| 系统 OCR（macOS Vision / Windows.Media.Ocr） | 不占体积，但两个平台结果不同，Windows 需要语言包，违反宪法 I，排除 |
| [ocr-rs](https://github.com/zibo-chen/rust-paddle-ocr)（MNN） | 构建时从个人仓库的 `dev` 标签下载预编译 MNN，供应链风险，排除 |
| [oar-ocr](https://github.com/GreatV/oar-ocr)（ONNX Runtime） | 功能完整，但链接 ONNX Runtime 后二进制增加约 20 MB，只作为基准 |
| **PaddleOCR 模型 + [rten](https://github.com/robertknight/rten)** | 纯 Rust 推理，二进制约增加 4 MB，可直接加载 ONNX，支持动态形状。**采用** |

模型用 PaddleOCR PP-OCRv6（Apache-2.0）：检测固定用 tiny（1.7 MiB），识别模型
可选，见下方「两种识别模型」。来源和校验和见 `crates/mcm-import/models/README.md`。

### 基准

测试图是 2522×878 的 archify 截图（`fixtures/archify-go-service.png`），共 26 段
有效文字。准确率用字符错误率（CER）计算，耗时是整条 OCR 流水线（检测 + 识别）
连续 5 次运行的稳定值。测试机为 Apple M 系列 Mac Studio。

| 模型 | 推理 | 完全正确 | CER | 耗时 |
|------|------|----------|-----|------|
| PP-OCRv5 mobile，默认缩放到 960 | ORT | 15/26 | 29.7% | 345 ms |
| PP-OCRv6 tiny，默认缩放到 960 | ORT | 2/26 | 54.1% | 70 ms |
| PP-OCRv5 mobile，原图分辨率 | ORT | 24/26 | 0.76% | 700 ms |
| PP-OCRv6 tiny，原图分辨率 | ORT | 24/26 | 0.51% | 240 ms |
| **PP-OCRv6 tiny，原图分辨率** | **rten** | **23/26** | **0.76%** | **315 ms** |
| PP-OCRv6 tiny 检测 + small 识别 | rten | 26/26 | 0% | 670 ms |

结论：

- **分辨率比模型更重要。** 截图里的字只有 12–14 px，按 PaddleOCR 默认把长边缩到
  960 会让小字几乎全部丢失。因此检测时不缩小，只在短边不足 736 px 时放大。
- tiny 剩下的错误集中在大小写和标点上（`PostgresQL`、`sqLc`、`discovery://` 少一个
  `/`），不影响结构识别。
- small 识别模型能做到全对，但多占 16 MiB、耗时翻倍，打进安装包会超出 25 MB 预算。
  所以两种都提供：tiny 内置作为默认，small 按需下载。

同一张图的整条导入流程（OCR、方框、箭头、生成大纲）耗时约 380 ms。截图和对应的
archify HTML 导入后，11 条箭头完全一致，由 `tests/html_fixture.rs` 守护。

## 两种识别模型

| | 快速（默认） | 高精度 |
|---|---|---|
| 识别模型 | PP-OCRv6 tiny，4.3 MiB | PP-OCRv6 small，20.2 MiB |
| 获取 | 编进二进制 | 导入对话框里点「下载」 |
| 本测试图 | 23/26，315 ms | 26/26，670 ms |

检测模型、方框和箭头识别两者完全相同，所以切换模型只会改变文字，不会改变结构。
`tests/accurate_model.rs` 检查了这一点：两种模型在测试图上得到的节点数、分组数和依赖数一致。
选择保存在偏好文件 `prefs.json` 的 `ocr_model` 字段（`fast` / `accurate`）。
导入 HTML 时不用 OCR，这个选项对 HTML 不起作用。

### 下载与校验

- **只在用户点击时联网**，符合宪法「网络能力是显式可选项，默认关闭」。
- 下载源按顺序尝试：ModelScope（`greatv/oar-ocr`，国内快）、GitHub
  （`GreatV/oar-ocr` v0.7.0 release）。一个源失败或校验不符就换下一个。
- 每个文件都校验大小和 SHA-256（固定在 `mcm_import::models::ACCURATE_FILES`），
  先写 `.part` 再改名，所以中断或被篡改的下载不会被当成已安装。
- 每次加载模型时会再校验一次摘要，文件事后损坏也会被拒绝，并提示重新下载。
- 存放位置：`<应用数据目录>/models/pp-ocrv6-small/`，对话框里会显示完整路径。
  离线环境可以手动把 `pp-ocrv6_small_rec.onnx` 和 `ppocrv6_dict.txt` 放进去。
- 对话框里可以删除已下载的模型，删除后自动切回快速模型。
- HTTP 客户端是 ureq，走系统 TLS（macOS Security.framework / Windows SChannel）
  和系统根证书，不引入额外的加密库。

## 识别方框和箭头的做法

截图是渲染出来的，不是拍照得到的，所以用确定性的像素分析就够了，不需要额外的
视觉模型。

1. **纸色**：取颜色直方图的众数。
2. **方框**：按行、按列扫描颜色一致的细笔画，允许 6 px 以内的间隙，所以虚线也能识别。
   方框的填充色是很粗的色带，会被排除。四条边的位置和颜色都对得上才算一个框，
   容许圆角。被连线压断的边框会重新接上。
3. **分组**：包含其他框的框就是分组，最小的外框作为父级。
4. **箭头**：去掉方框内部、分组边框和文字之后，剩下的墨迹按 6 px 邻域连成连线。
   连线碰到的方框就是端点，墨迹明显更多（三角形箭头）的一端是终点。
   分组边框和文字只按各自的颜色及其抗锯齿过渡色擦除，所以从标题下方穿过、
   或者跨过分组边框的连线不会被切断。

## 调试

```bash
# 输出大纲，并把识别出的文字（蓝）、方框（红）、分组（紫）、箭头画到图上
cargo run --release -p mcm-import --example import_image -- 图.png overlay.png
# 只看 OCR 结果和耗时
cargo run --release -p mcm-import --example ocr_dump -- 图.png
# archify HTML
cargo run --release -p mcm-import --example import_html -- 图.html
```

高精度模型的测试默认跳过（模型不入库），下载后手动运行：

```bash
MCM_ACCURATE_MODEL_DIR=<模型目录> cargo test -p mcm-import -- --ignored
cargo test -p mcm-app real_mirrors -- --ignored   # 真实访问两个下载源
```

`cargo test` 在 debug 构建下运行。工作区 `Cargo.toml` 对 rten 和图像解码相关依赖
单独开启了优化，所以真实截图的端到端测试约 1 秒就能跑完。

## 已知限制

- 只识别矩形框（含圆角）。菱形、圆形、泳道标题栏暂不识别，里面的文字会作为未归属文字保留。
- 曲线或者从框中间穿过的连线可能匹配到错误的端点。导入后请在「依赖网络」视图里核对。
- 文字只识别水平方向。
- archify lifecycle 图中，主轨道上的阶段顺序没有带连线标注，只导入显式画出的转移。

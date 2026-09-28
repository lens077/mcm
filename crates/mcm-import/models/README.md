# OCR 模型

本目录的文件在编译期经 `include_bytes!` 嵌入二进制，是默认的「快速」模型，
运行时不联网、不读外部文件。

| 文件 | 用途 | 大小 | SHA-256 |
|------|------|------|---------|
| `pp-ocrv6_tiny_det.onnx` | 文字检测（DB） | 1.7 MiB | `193bab7a04fca699a6c82e6abb5b81bdb28177f0abd4062552b04908dafb19f8` |
| `pp-ocrv6_tiny_rec.onnx` | 文字识别（CTC，中英日） | 4.3 MiB | `9ef676d6ed3c88256a2d92c640c44f25b0c40947e111b14b8be8f594091563e6` |
| `ppocrv6_tiny_dict.txt` | 识别字典（6904 字符） | 27 KiB | `c5cbe34ef40c29c4df07ed012bf96569cb69a2d2a01a07027e9f13cb832bd9cd` |

## 高精度模型（不在本目录）

用户在导入对话框里点击下载后才获取，文件摘要固定在 `src/models.rs` 的
`ACCURATE_FILES`：

| 文件 | 用途 | 字节数 | SHA-256 |
|------|------|--------|---------|
| `pp-ocrv6_small_rec.onnx` | 文字识别（CTC，18708 字符） | 21 159 378 | `5435fd747c9e0efe15a96d0b378d5bd157e9492ed8fd80edf08f30d02fa24634` |
| `ppocrv6_dict.txt` | 识别字典 | 74 947 | `b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d` |

## 来源与许可

PaddlePaddle 的 [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) PP-OCRv6 tiny，
Apache-2.0。ONNX 文件取自 [oar-ocr v0.7.0 release](https://github.com/GreatV/oar-ocr/releases/tag/v0.7.0)，
与 PaddleX 官方 `PP-OCRv6_*_onnx_infer.tar` 同源；ModelScope `greatv/oar-ocr`
仓库发布的是同一批文件，摘要一致。

## 更换模型

1. 替换文件并更新上表的 SHA-256（`shasum -a 256 models/*`）。
2. 识别模型的输出类别数必须等于「字典行数 + 2」（CTC blank 与空格），
   不一致时 `OcrEngine` 会报错而不是输出乱码。
3. 跑 `cargo test -p mcm-import`：`tests/image_fixture.rs` 以真实截图为基准，
   识别退化会直接失败。
4. 按 `docs/image-import.md` 的方法复测准确率与耗时，把结果写回该文档。

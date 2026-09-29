---
name: Sumery Pyxis
brand: ../work/docs/brand/DESIGN.md
colors:
  pyxis: "#84cc16"
  pyxis-light: "#d9f99d"
  accent-light: "#3f6212"
  primary-fill-light: "#84cc16"
  accent-dark: "#a3e635"
  on-primary: "#1b1547"
---

# Sumery Pyxis 设计说明

品牌、命名、图标外壳、页脚与定价原则见 Sumery 品牌系统（`work/docs/brand/DESIGN.md`）。本文件只定义 Pyxis 的强调色、界面语言与组件。

## 名称

| 场合 | 写法 |
| --- | --- |
| 可搜索名（安装包、Finder、窗口标题、官网标题） | `Sumery Pyxis` |
| 短名（菜单栏、侧栏 wordmark） | `Pyxis` |
| 侧栏 kicker | `SUMERY · PYXIS` |
| 中文首次出现 | `希曼 Sumery Pyxis（罗盘座）` |
| 工程名（不改） | 仓库 `mcm`、crate `mcm-*`、可执行文件 `mcm-app`、文件格式 `.mcm`、环境变量 `MCM_*`、域名 `mcm.apikv.com` |

`src/identity.test.ts` 钉住上表里操作系统能看到的字段。

## 图标

`design/` 里是审阅通过的品牌图像的派生副本（源在 Libra 仓库 `docs/brand/icons/source/pyxis-{light,dark}.png`）：节点树——根节点是黄绿罗盘环 + 竖菱指针，两条线分出两个子节点；节点星在根节点顶部。浅色纸底 / 深色板岩底两版跟随系统外观，Dock 用深色版。不要手改 PNG；改图标就重出源图，再在 Libra 仓库跑 `python3 tools/brand-icon/build-image-icons.py pyxis && bash tools/brand-icon/install.sh pyxis`，它会：

- `design/icon-1024.png`（= 深色）→ `tauri icon` → `src-tauri/icons/`（只保留仓库已跟踪的 6 个文件）
- `favicon-light.png` / `favicon-dark.png` / `favicon.png` → `public/`（应用侧栏标记与 WebView 页签）与 `site/public/`（官网），用 `<picture>` / `<link media>` 跟随系统外观
- `design/tray-iconTemplate*.png`：Pyxis 暂无菜单栏图标，模板留作备用

## 颜色

黄绿本身在浅底上不可读（白底 1.98:1），所以分成三种用途：

| token | 浅色 | 深色 | 用途 |
| --- | --- | --- | --- |
| `--accent` | `#3f6212` | `#a3e635` | 强调文字、描边、焦点环 |
| `--primary-fill` | `#84cc16` | `#a3e635` | 主按钮底色 |
| `--on-primary` | `#1b1547` | `#1b1547` | 主按钮文字（黄绿上必须用深字） |

`--accent` 取 `#3f6212` 是因为画布 `#f3efe4` 比白底暗：`#4d7c0f` 在画布上只有 4.35:1，`#3f6212` 为 6.16:1。

`--role-*` 场景配色（依赖连线、里程碑、完成、错误）是数据可视化的语义色，不随品牌色相换；Visio 导出的连线颜色同理。

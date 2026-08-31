# MCM 宣传站点

Astro 7 静态站点，产物约 24 KB。

**线上地址**：https://mcm.apikv.com

## 本地开发

```bash
make site-dev       # 开发服务器
make site-build     # 构建到 site/dist
make site-preview   # 预览构建产物
```

`site/` 自成一个 pnpm 工作区（见 `pnpm-workspace.yaml`），与仓库根解耦。
**不要把它并入根工作区**：Astro 依赖会写进根 `pnpm-lock.yaml`，而该文件在
release 工作流的触发白名单内——改一次落地页就会平白发一个安装包毫无变化的
新版本。

## 部署（node1）

站点跑在 node1 上，经 Pangolin/Traefik 暴露。与同机的 `homepage` 同一模式：
挂 `pangolin_frontend` 网络、不占宿主端口。

```bash
make site-build
tar -C site/dist -czf - . | ssh node1 'tar -C /home/docker/mcm-site/site -xzf -'
ssh node1 'cd /home/docker/mcm-site && docker compose restart'
```

服务端布局：

| 位置 | 内容 |
|---|---|
| `/home/docker/mcm-site/compose.yml` | nginx 容器定义 |
| `/home/docker/mcm-site/site/` | 静态产物挂载目录 |

Pangolin 侧（已配置好，无需重复操作）：

- 资源 `mcm.apikv.com`，resourceId `45`
- target 指向容器名 `mcm-site:80`，由 Docker DNS 解析
- **`sso: false`** —— 新建资源默认开启 SSO，会把访客重定向到登录页。
  宣传页必须公开，所以显式关掉了。

## 待办

- [ ] **Cloudflare**：接入 CDN 与缓存（用户要求先记录，暂不实施）
- [ ] **GitHub Pages**：当前不可行。仓库是私有的，而 Free 账户的私仓
      无法启用 Pages（需 Pro 及以上）。若希望公开访问，两条路：
      把仓库转为 public，或升级账户套餐。
- [ ] 补产品截图（现在全是文字，四视图应该有图更有说服力）
- [ ] 站点内容目前硬编码在 `index.astro`，若要多页面再抽布局组件

## 内容原则

页面上的每个数字都来自仓库内实测，并标注了对照预算——不写没有依据的宣传话术。
Visio 那段写了真实的踩坑经过而非只讲成功，这是刻意的：目标读者会看实现。

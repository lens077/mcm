// @ts-check
import { defineConfig } from "astro/config";
import process from "node:process";

// 站点同时部署到两处，路径前缀不同：
//   node1（mcm.apikv.com）      根路径 /
//   GitHub Pages（/mcm 子路径） 需要 base=/mcm
//
// 所以 site 与 base 都从环境变量取，构建时由各自的工作流注入。
// 模板里的资源路径必须用 import.meta.env.BASE_URL 拼接——Astro 不会
// 自动给 <img src="/x.png"> 这类绝对路径加前缀，写死会让子路径部署下
// 图片全部 404。
const site = process.env.SITE_URL ?? "https://mcm.apikv.com";
const base = process.env.SITE_BASE ?? "/";

export default defineConfig({
  site,
  base,
  output: "static",
  build: {
    // 生成 about/index.html 而非 about.html，静态服务器无需额外重写规则
    format: "directory",
  },
});

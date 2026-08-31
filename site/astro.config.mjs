// @ts-check
import { defineConfig } from "astro/config";

// 纯静态输出：落地页没有服务端逻辑，产物可以直接由任意静态服务器托管，
// 也便于通过 Pangolin 这类反向代理暴露。
export default defineConfig({
  site: "https://pangolin.apikv.com",
  output: "static",
  build: {
    // 生成 about/index.html 而非 about.html，静态服务器无需额外重写规则
    format: "directory",
  },
});

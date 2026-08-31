#!/usr/bin/env node
// 为宣传站点生成产品截图。
//
// 截图必须是真实渲染，不能是另画的示意图。所以：
//   1. 场景数据由 mcm-core 产出（examples/dump_scenes），与应用同一代码路径：
//      解析 → 校验 → 布局 → 场景投影
//   2. 由真实的前端产物（dist/）渲染，用的就是应用自己的 canvas 渲染器
//   3. 这里只注入一个 __TAURI_INTERNALS__ 桩，把上述 JSON 喂给 IPC 层，
//      不改动任何界面代码
//
//     node scripts/capture-screens.mjs <场景目录> <输出目录>

import { spawn } from "node:child_process";
import { existsSync, readFileSync, mkdirSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { chromium } from "playwright";

// addInitScript 传入的函数体由 Playwright 序列化后在浏览器里执行，
// 那里有 window；本文件其余部分仍是 Node。
/* global window */

const sceneDir = process.argv[2] || "/tmp/scenes";
const outDir = process.argv[3] || "site/public/shots";
const projectRoot = path.resolve(import.meta.dirname, "..");
const distDir = path.join(projectRoot, "dist");

if (!existsSync(distDir)) {
  console.error("找不到 dist/，请先运行 pnpm build");
  process.exit(2);
}
mkdirSync(path.join(projectRoot, outDir), { recursive: true });

const readJson = (name) =>
  JSON.parse(readFileSync(path.join(sceneDir, name), "utf8"));

const payload = {
  session: readJson("session.json"),
  issues: readJson("issues.json"),
  outline: readJson("outline.json"),
  scenes: {
    wbs: readJson("scene-wbs.json"),
    graph: readJson("scene-graph.json"),
    timeline: readJson("scene-timeline.json"),
    milestones: readJson("scene-milestones.json"),
  },
};

const views = [
  { key: "wbs", label: "任务分解" },
  { key: "graph", label: "依赖网络" },
  { key: "timeline", label: "时间线" },
  { key: "milestones", label: "里程碑" },
];

// 静态服务 dist/，让真实前端跑起来
const server = spawn(
  "node",
  [
    "-e",
    `
    const http=require('http'),fs=require('fs'),p=require('path');
    const root=${JSON.stringify(distDir)};
    const types={'.html':'text/html','.js':'text/javascript','.css':'text/css','.svg':'image/svg+xml'};
    http.createServer((req,res)=>{
      let f=p.join(root,req.url==='/'?'index.html':req.url.split('?')[0]);
      if(!fs.existsSync(f)) f=p.join(root,'index.html');
      res.writeHead(200,{'Content-Type':types[p.extname(f)]||'application/octet-stream'});
      fs.createReadStream(f).pipe(res);
    }).listen(4399);
    `,
  ],
  { stdio: "ignore" },
);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
await sleep(900);

const browser = await chromium.launch();
try {
  for (const view of views) {
    const page = await browser.newPage({
      viewport: { width: 1440, height: 900 },
      deviceScaleFactor: 2, // 二倍图，站点上放大也清晰
    });

    // 注入 Tauri 桩：把真实核心产出的数据交给未经改动的 IPC 层
    await page.addInitScript(
      ({ data, current }) => {
        const state = { view: current };
        window.__TAURI_INTERNALS__ = {
          invoke: (cmd, args) => {
            switch (cmd) {
              case "session_state":
              case "session_new":
                return Promise.resolve(data.session);
              case "issues_get":
                return Promise.resolve(data.issues);
              case "outline_text_get":
                return Promise.resolve(data.outline);
              case "scene_get": {
                // 按前端实际请求的视图返回，映射 ViewKind -> 转储文件名
                const map = {
                  wbs: "wbs",
                  dep_graph: "graph",
                  depgraph: "graph",
                  DepGraph: "graph",
                  Wbs: "wbs",
                  Timeline: "timeline",
                  Milestones: "milestones",
                  timeline: "timeline",
                  milestones: "milestones",
                };
                const key = map[args?.view] ?? state.view;
                return Promise.resolve(data.scenes[key] ?? data.scenes.wbs);
              }
              case "app_close_check":
                return Promise.resolve({ dirty: false });
              case "search":
                return Promise.resolve([]);
              case "prefs_get":
                return Promise.resolve({ recent_files: [], view_state: {} });
              case "file_check_external":
                return Promise.resolve({ status: "none" });
              default:
                return Promise.resolve(null);
            }
          },
        };
      },
      { data: payload, current: view.key },
    );

    await page.goto("http://localhost:4399/", { waitUntil: "networkidle" });
    await sleep(600);

    // 真实点击视图标签，而不是靠注入状态——这样截出来的就是用户看到的
    const tab = page.getByRole("button", { name: view.label, exact: true });
    if (await tab.count()) {
      await tab.first().click();
    } else {
      console.warn(`  ⚠️ 未找到视图标签「${view.label}」`);
    }
    await sleep(1000); // 等画布重绘

    const file = path.join(projectRoot, outDir, `${view.key}.png`);
    await page.screenshot({ path: file });
    console.log(`  ${view.label} -> ${path.relative(projectRoot, file)}`);
    await page.close();
  }
} finally {
  await browser.close();
  server.kill();
}

#!/usr/bin/env node
// 打包安装包，并在 macOS 权限被拒时自动降级。
//
// 背景：Tauri 打 .dmg 时会调用 bundle_dmg.sh，后者用 osascript 驱动 Finder
// 美化磁盘映像窗口（背景图、图标位置）。这需要「自动化 / System Events」权限。
// 用户拒绝授权时脚本以退出码 64 失败，整个打包随之失败 —— 尽管 .app 早已
// 成功产出，且 .dmg 本身并不依赖那段美化。
//
// bundle_dmg.sh 自带 --skip-jenkins 开关跳过美化，Tauri 在检测到 CI 环境变量
// 时会自动传入。所以降级路径就是：带 CI=true 重跑。
//
// 这里刻意先走正常路径：授权可用时仍然产出带窗口布局的 .dmg，只有确认踩到
// 这个特定失败时才降级，绝不掩盖其他构建错误。

import { spawn } from "node:child_process";
import { existsSync, readdirSync, rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const projectRoot = path.resolve(import.meta.dirname, "..");
const bundleRoot = path.join(projectRoot, "target", "release", "bundle");
const passthroughArgs = process.argv.slice(2);

/** 运行 tauri build，边输出边捕获，返回退出码与合并后的输出。 */
function runTauriBuild(extraEnv = {}) {
  return new Promise((resolve) => {
    const child = spawn("pnpm", ["tauri", "build", ...passthroughArgs], {
      cwd: projectRoot,
      env: { ...process.env, ...extraEnv },
      shell: process.platform === "win32",
    });

    let combined = "";
    const capture = (stream, sink) => {
      stream.on("data", (chunk) => {
        const text = chunk.toString();
        combined += text;
        sink.write(text);
      });
    };
    capture(child.stdout, process.stdout);
    capture(child.stderr, process.stderr);

    child.on("close", (code) => resolve({ code: code ?? 1, output: combined }));
    child.on("error", (error) => resolve({ code: 1, output: String(error) }));
  });
}

/**
 * 这次失败是否正是「DMG 美化因权限被拒」？
 *
 * 决定性证据是 AppleScript 的 -1743（errAEEventNotPermitted，
 * 「未获得授权将 Apple 事件发送给 Finder」）。但 Tauri 默认会吞掉脚本的
 * stderr，只留一句笼统的 "failed to run bundle_dmg.sh"，所以两种特征都认。
 */
function isDmgPermissionFailure(output) {
  if (output.includes("-1743") || output.includes("Failed running AppleScript")) {
    return true;
  }
  return output.includes("bundle_dmg.sh") && output.includes("failed to run");
}

/** 清理 bundle_dmg.sh 早退时遗留的读写中间映像。 */
function cleanIntermediates() {
  for (const dir of ["macos", "dmg"]) {
    const full = path.join(bundleRoot, dir);
    if (!existsSync(full)) continue;
    for (const entry of readdirSync(full)) {
      if (/^rw\.\d+\./.test(entry)) {
        rmSync(path.join(full, entry), { force: true, recursive: true });
      }
    }
  }
}

/** .app 是否已成功产出（用于确认失败只发生在 DMG 阶段）。 */
function appBundleExists() {
  const macosDir = path.join(bundleRoot, "macos");
  if (!existsSync(macosDir)) return false;
  return readdirSync(macosDir).some((entry) => entry.endsWith(".app"));
}

const first = await runTauriBuild();

if (first.code === 0) {
  process.exit(0);
}

// 非 macOS，或失败原因与 DMG 权限无关 —— 原样传递失败，不掩盖真实问题。
if (process.platform !== "darwin" || !isDmgPermissionFailure(first.output)) {
  process.exit(first.code);
}

if (!appBundleExists()) {
  console.error("\n应用本体未能产出，失败并非仅发生在 DMG 阶段，不做降级。");
  process.exit(first.code);
}

console.error(`
──────────────────────────────────────────────────────────────
  DMG 窗口美化需要 macOS「自动化」权限，本次未获授权
  （AppleScript -1743：未获得授权将 Apple 事件发送给 Finder）。

  自动降级：跳过美化重新打包。
  产出的 .dmg 仍可正常挂载与安装，仅缺少自定义背景与图标布局。

  若想要完整外观：系统设置 → 隐私与安全性 → 自动化，
  允许终端（或你的 IDE）控制「Finder」，再重跑。
  注意授权是按「发起进程」归属的：在 IDE 内置终端里跑，需要授权那个 IDE。
──────────────────────────────────────────────────────────────
`);

// 失败那次会遗留一个读写中间映像（rw.<pid>.*.dmg）。它不是交付物，
// 留着会污染体积门槛，也会随每次失败堆积。
cleanIntermediates();

// CI=true 让 Tauri 给 bundle_dmg.sh 传 --skip-jenkins，跳过 AppleScript。
const second = await runTauriBuild({ CI: "true" });

if (second.code !== 0) {
  console.error("\n降级打包同样失败，请查看上方输出。");
  process.exit(second.code);
}

console.error("\n✅ 已完成打包（DMG 为降级外观）。\n");

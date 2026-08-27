#!/usr/bin/env node
// Cold-start budget probe (spec SC-003: ≤ 2s at P95, plan.md §Performance Goals).
//
// The release binary is launched with `--selftest`, which exercises the same
// core paths the UI needs before it can paint, then exits. That gives a stable,
// headless proxy for "launch to interactive" that works identically on macOS
// and Windows CI.

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const RUNS = Number(process.env.MCM_STARTUP_RUNS ?? 20);
const BUDGET_MS = Number(process.env.MCM_STARTUP_BUDGET_MS ?? 2000);

const projectRoot = path.resolve(import.meta.dirname, "..");
const binaryName = process.platform === "win32" ? "mcm-app.exe" : "mcm-app";
// Cargo puts workspace artifacts in the root target directory; the per-crate
// path is the fallback for non-workspace builds.
const candidates = [
  path.join(projectRoot, "target", "release", binaryName),
  path.join(projectRoot, "src-tauri", "target", "release", binaryName),
];
const binary = candidates.find((candidate) => existsSync(candidate));

if (!binary) {
  console.error(`找不到发行版二进制，已查找：\n  ${candidates.join("\n  ")}`);
  console.error("请先运行：cargo build --release -p mcm-app");
  process.exit(2);
}

/** Returns the p-th percentile of a numeric sample. */
function percentile(values, p) {
  const sorted = [...values].sort((a, b) => a - b);
  if (sorted.length === 0) return 0;
  const rank = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.min(Math.max(rank, 0), sorted.length - 1)];
}

const samples = [];
// One discarded warm-up run keeps page-cache effects out of the sample.
spawnSync(binary, ["--selftest"], { stdio: "ignore" });

for (let index = 0; index < RUNS; index += 1) {
  const started = process.hrtime.bigint();
  const result = spawnSync(binary, ["--selftest"], { stdio: "ignore" });
  const elapsedMs = Number(process.hrtime.bigint() - started) / 1e6;

  if (result.status !== 0) {
    console.error(`第 ${index + 1} 次运行失败（退出码 ${result.status}）`);
    process.exit(1);
  }
  samples.push(elapsedMs);
}

const p95 = percentile(samples, 95);
const median = percentile(samples, 50);
const worst = Math.max(...samples);

console.log(`冷启动测量（${RUNS} 次，${process.platform}）`);
console.log(`  中位数 ${median.toFixed(1)} ms`);
console.log(`  P95    ${p95.toFixed(1)} ms`);
console.log(`  最差   ${worst.toFixed(1)} ms`);
console.log(`  预算   ${BUDGET_MS} ms`);

if (p95 > BUDGET_MS) {
  console.error(`\n❌ 冷启动 P95 ${p95.toFixed(1)} ms 超出预算 ${BUDGET_MS} ms`);
  process.exit(1);
}
console.log(`\n✅ 冷启动 P95 在预算内`);

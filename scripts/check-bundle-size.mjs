#!/usr/bin/env node
// Installer size budget (plan.md §Constraints: ≤ 25MB, 宪法 II).
//
// Runs after `pnpm tauri build` and fails CI when a bundle exceeds the budget,
// so packaging bloat cannot creep in unnoticed.

import { existsSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const BUDGET_MB = Number(process.env.MCM_BUNDLE_BUDGET_MB ?? 25);
const projectRoot = path.resolve(import.meta.dirname, "..");
const bundleRoot = path.join(projectRoot, "target", "release", "bundle");

/** Installer extensions worth measuring, per platform. */
const INSTALLER_EXTENSIONS = new Set([".dmg", ".app", ".msi", ".exe", ".deb", ".AppImage"]);

function* walk(dir) {
  if (!existsSync(dir)) return;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      // `.app` is a directory bundle; measure it as one artifact.
      if (path.extname(entry.name) === ".app") {
        yield full;
        continue;
      }
      yield* walk(full);
    } else if (INSTALLER_EXTENSIONS.has(path.extname(entry.name))) {
      yield full;
    }
  }
}

/** Total size of a file or directory, in bytes. */
function sizeOf(target) {
  const stats = statSync(target);
  if (!stats.isDirectory()) return stats.size;
  let total = 0;
  for (const entry of readdirSync(target, { withFileTypes: true })) {
    total += sizeOf(path.join(target, entry.name));
  }
  return total;
}

const artifacts = [...walk(bundleRoot)];

if (artifacts.length === 0) {
  console.log(`未找到安装包（${bundleRoot}）；跳过体积检查。`);
  console.log("提示：先运行 pnpm tauri build");
  process.exit(0);
}

let worstMb = 0;
console.log(`安装包体积检查（预算 ${BUDGET_MB} MB）`);
for (const artifact of artifacts) {
  const mb = sizeOf(artifact) / (1024 * 1024);
  worstMb = Math.max(worstMb, mb);
  const marker = mb > BUDGET_MB ? "❌" : "✅";
  console.log(`  ${marker} ${mb.toFixed(1).padStart(6)} MB  ${path.relative(projectRoot, artifact)}`);
}

if (worstMb > BUDGET_MB) {
  console.error(`\n❌ 最大安装包 ${worstMb.toFixed(1)} MB 超出预算 ${BUDGET_MB} MB`);
  process.exit(1);
}
console.log(`\n✅ 全部安装包在 ${BUDGET_MB} MB 预算内`);

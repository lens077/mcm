#!/usr/bin/env node
// 校验 release 产物清单是否覆盖该平台的声明。
//
//   node scripts/assert-artifacts.mjs --platform macos --dir out
//   node scripts/assert-artifacts.mjs --platform windows a.msi b-setup.exe
//
// 不传 --dir 时把位置参数当作产物文件名，用于离线验证断言逻辑本身：
// 不需要真的打包，用一组假名字就能复现 v0.2.0 那次「Windows 缺 .msi、
// macOS 只有 aarch64.dmg」的回归。
//
// 声明（与 tauri.conf.json 的 bundle.targets = "all" 和 release.yml 对应）：
//   Windows  必须同时有 WiX .msi 与 NSIS -setup.exe
//   macOS    必须覆盖 Intel 与 Apple Silicon：universal 一个包即可，
//            否则 aarch64 与 x64 各要一份

import { existsSync, readdirSync } from "node:fs";
import process from "node:process";

/** Tauri 的 DMG 命名是 <产品>_<版本>_<arch>.dmg。 */
function dmgArch(name) {
  const match = /_(aarch64|x64|universal)\.dmg$/i.exec(name);
  return match ? match[1].toLowerCase() : null;
}

function checkWindows(names) {
  const problems = [];
  if (!names.some((name) => name.endsWith(".msi"))) {
    problems.push("缺少 WiX .msi 安装包");
  }
  if (!names.some((name) => name.endsWith("-setup.exe"))) {
    problems.push("缺少 NSIS .exe 安装包");
  }
  return problems;
}

function checkMacos(names) {
  const dmgs = names.filter((name) => name.endsWith(".dmg"));
  if (dmgs.length === 0) {
    return ["没有任何 .dmg"];
  }
  const arches = new Set(dmgs.map(dmgArch).filter(Boolean));
  // universal 同时覆盖两种架构；否则必须两种都产出。
  if (arches.has("universal") || (arches.has("aarch64") && arches.has("x64"))) {
    return [];
  }
  return [
    `DMG 架构不完整，实际只有 [${[...arches].join(", ") || "名称无法解析"}]：` +
      `${dmgs.join(", ")}；需要 universal，或 aarch64 与 x64 各一份`,
  ];
}

const RULES = { macos: checkMacos, windows: checkWindows };

function parseArgs(argv) {
  let platform = null;
  let dir = null;
  const files = [];
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === "--platform") {
      platform = argv[(i += 1)];
    } else if (argv[i] === "--dir") {
      dir = argv[(i += 1)];
    } else {
      files.push(argv[i]);
    }
  }
  return { platform, dir, files };
}

const { platform, dir, files } = parseArgs(process.argv.slice(2));
const check = RULES[platform];

if (!check) {
  console.error(
    `用法：assert-artifacts.mjs --platform <${Object.keys(RULES).join("|")}> [--dir 目录 | 文件名...]`,
  );
  process.exit(2);
}

let names = files;
if (dir) {
  if (!existsSync(dir)) {
    console.error(`❌ 产物目录不存在：${dir}`);
    process.exit(1);
  }
  names = readdirSync(dir).filter((name) => !name.startsWith("."));
}

const problems = check(names);

if (problems.length > 0) {
  console.error(`❌ ${platform} 产物不完整（来源：${dir ?? "命令行参数"}）`);
  for (const problem of problems) {
    console.error(`   - ${problem}`);
  }
  console.error(`   实际产物：${names.join(", ") || "（空）"}`);
  process.exit(1);
}

console.log(`✅ ${platform} 产物齐全：${names.join(", ")}`);

#!/usr/bin/env node
// 递增版本号，同步写入所有声明版本的位置。
//
//   node scripts/bump-version.mjs [patch|minor|major] [--dry-run]
//
// 版本在两处声明，必须一起改，否则 tauri 打出来的包名与 crate 版本会对不上：
//   - Cargo.toml          [workspace.package] version
//   - src-tauri/tauri.conf.json   version
//
// Cargo.lock 里的工作区成员版本由调用方用 `cargo update -w` 刷新——用 cargo
// 自己改比在这里做正则替换可靠。
//
// 成功时把新版本号打到 stdout（仅此一行），便于 CI 直接取用。

import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const LEVELS = new Set(["patch", "minor", "major"]);

const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run");
const level = args.find((arg) => LEVELS.has(arg)) ?? "patch";

const projectRoot = path.resolve(import.meta.dirname, "..");
const cargoPath = path.join(projectRoot, "Cargo.toml");
const tauriPath = path.join(projectRoot, "src-tauri", "tauri.conf.json");

/** 只接受 x.y.z：预发布/构建元数据不在自动递增的范围内。 */
function parse(version) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(version);
  if (!match) {
    throw new Error(`版本号必须是 x.y.z，实际为 ${version}`);
  }
  return { major: +match[1], minor: +match[2], patch: +match[3] };
}

export function nextVersion(current, bump) {
  const { major, minor, patch } = parse(current);
  switch (bump) {
    case "major":
      return `${major + 1}.0.0`;
    case "minor":
      return `${major}.${minor + 1}.0`;
    default:
      return `${major}.${minor}.${patch + 1}`;
  }
}

/** 读取 [workspace.package] 段里的 version，避免误取别处同名键。 */
function readCargoVersion(text) {
  const section = /\[workspace\.package\][\s\S]*?(?=\n\[|$)/.exec(text);
  if (!section) throw new Error("Cargo.toml 中找不到 [workspace.package] 段");
  const match = /^\s*version\s*=\s*"([^"]+)"/m.exec(section[0]);
  if (!match) throw new Error("[workspace.package] 段中找不到 version");
  return match[1];
}

function writeCargoVersion(text, from, to) {
  const section = /\[workspace\.package\][\s\S]*?(?=\n\[|$)/.exec(text);
  const updated = section[0].replace(
    /^(\s*version\s*=\s*)"[^"]+"/m,
    `$1"${to}"`,
  );
  const result = text.slice(0, section.index) + updated + text.slice(section.index + section[0].length);
  if (!result.includes(`"${to}"`)) {
    throw new Error(`Cargo.toml 版本未能从 ${from} 更新到 ${to}`);
  }
  return result;
}

const cargoText = readFileSync(cargoPath, "utf8");
const current = readCargoVersion(cargoText);
const next = nextVersion(current, level);

const tauriText = readFileSync(tauriPath, "utf8");
const tauri = JSON.parse(tauriText);
if (tauri.version !== current) {
  throw new Error(
    `版本声明不一致：Cargo.toml 为 ${current}，tauri.conf.json 为 ${tauri.version}。` +
      "请先手工对齐再递增。",
  );
}

if (!dryRun) {
  writeFileSync(cargoPath, writeCargoVersion(cargoText, current, next));
  // 保留原有缩进与末尾换行，避免产生无关 diff
  tauri.version = next;
  writeFileSync(tauriPath, `${JSON.stringify(tauri, null, 2)}\n`);
}

process.stderr.write(`${current} -> ${next} (${level}${dryRun ? ", dry-run" : ""})\n`);
process.stdout.write(`${next}\n`);

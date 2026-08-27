# MCM — 常用命令
#
# 直接运行 `make` 查看全部目标。
#
# 这个 Makefile 不只是命令别名表，它编码了两条真实的依赖关系：
#
#   1. dist/ 必须先于任何编译 mcm-app 的 cargo 命令
#      Tauri 的 generate_context! 宏在编译期嵌入前端产物，而 dist/ 被
#      gitignore。干净检出时若先跑 cargo，会以
#      "The frontendDist configuration is set to ../dist but this path
#      doesn't exist" 失败 —— CI 曾栽在这里。
#
#   2. node_modules 依赖 lockfile
#      lockfile 变了就自动重装，不必手工记着。
#
# Windows 上默认没有 make。全部目标都有等价的 pnpm/cargo 命令，
# 见每条规则的实现，或直接用 pnpm 脚本。

SHELL := /bin/bash
.DEFAULT_GOAL := help

# 前端产物的输入：任一变化都应触发重新构建
FRONTEND_SRC := $(shell find src -type f 2>/dev/null)
FRONTEND_CONF := index.html vite.config.ts tsconfig.json package.json
DIST_ENTRY := dist/index.html

# 发行版二进制（smoke / 冷启动测量都用它）
RELEASE_BIN := target/release/mcm-app

.PHONY: help install dev build build-universal bundle \
        fmt fmt-check lint lint-rs lint-ci test test-rs test-web \
        bench smoke gate ci \
        check-bundle measure-startup fixtures \
        clean clean-dist distclean verify-clean-checkout

# ─────────────────────────────── 帮助 ───────────────────────────────

help: ## 显示本帮助
	@echo ""
	@echo "  MCM 常用命令"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}'
	@echo ""
	@echo "  首次使用：make install && make dev"
	@echo "  提交之前：make gate"
	@echo ""

# ─────────────────────────────── 依赖 ───────────────────────────────

install: node_modules ## 安装依赖（lockfile 变化时自动重装）

# 目录时间戳不可靠，装完 touch 一下作为标记
node_modules: package.json pnpm-lock.yaml
	pnpm install --frozen-lockfile
	@touch node_modules

# ─────────────────────────────── 启动 ───────────────────────────────

dev: node_modules ## 启动桌面应用（开发模式，热更新）
	pnpm tauri dev

web: node_modules ## 仅启动前端 Vite 开发服务器（无 Tauri 外壳，IPC 不可用）
	pnpm dev

# ─────────────────────────────── 构建 ───────────────────────────────

# 前端产物。cargo 目标都依赖它，见文件头说明。
$(DIST_ENTRY): node_modules $(FRONTEND_SRC) $(FRONTEND_CONF)
	pnpm build

build: $(DIST_ENTRY) ## 构建前端产物（cargo 编译 mcm-app 的前置条件）

bundle: $(DIST_ENTRY) ## 打包本平台安装包（macOS: .app/.dmg，Windows: .msi）
	pnpm bundle

build-universal: $(DIST_ENTRY) ## 打包 macOS 双架构安装包
	pnpm build:mac-universal

$(RELEASE_BIN): $(DIST_ENTRY)
	cargo build --release -p mcm-app

# ─────────────────────────────── 质量门 ───────────────────────────────

fmt: ## 格式化 Rust 代码
	cargo fmt --all

fmt-check: ## 检查 Rust 格式（不修改文件）
	cargo fmt --all -- --check

lint-rs: $(DIST_ENTRY) ## Clippy，零警告
	cargo clippy --workspace --all-targets -- -D warnings

lint: node_modules ## ESLint，零警告
	pnpm lint

lint-ci: ## 校验 GitHub Actions 工作流（需 brew install actionlint）
	actionlint

test-rs: $(DIST_ENTRY) ## Rust 全量测试（单元 / 属性 / 契约）
	cargo test --workspace

test-web: node_modules ## 前端测试
	pnpm test

test: test-rs test-web ## 全部测试

bench: ## 性能预算（超预算即 panic）
	cargo bench -p mcm-core

smoke: $(DIST_ENTRY) ## 端到端场景冒烟（双平台通用的共享清单）
	pnpm smoke

check-bundle: ## 校验安装包体积 ≤ 25MB（需先 make bundle）
	pnpm check:bundle

measure-startup: $(RELEASE_BIN) ## 测量冷启动 P95 ≤ 2s
	pnpm measure:startup

# 与 .github/workflows/ci.yml 保持一致；顺序即 CI 的执行顺序
gate: build fmt-check lint-rs test-rs bench lint test-web smoke ## 提交前的完整质量门
	@echo ""
	@echo "  ✅ 质量门全部通过"
	@echo ""

ci: gate bundle check-bundle measure-startup ## 完整复现 CI（含打包与体积/启动预算）
	@echo ""
	@echo "  ✅ CI 全流程通过"
	@echo ""

# ─────────────────────────────── 工具 ───────────────────────────────

fixtures: ## 重新生成性能测试夹具
	cargo run -p mcm-core --bin gen_fixture -- 1000 fixtures/perf/plan-1000.mcm
	cargo run -p mcm-core --bin gen_fixture -- 5000 fixtures/perf/plan-5000.mcm

# CI 是干净检出，本地却常有残留的 dist/，两者行为会不一致。
# 这个目标专门复现干净检出，用来验证构建顺序没被破坏。
verify-clean-checkout: ## 模拟 CI 干净检出（先删 dist/ 再跑质量门）
	@echo "→ 删除 dist/，模拟干净检出"
	@rm -rf dist
	@$(MAKE) --no-print-directory gate

# ─────────────────────────────── 清理 ───────────────────────────────

clean-dist: ## 删除前端产物
	rm -rf dist

clean: clean-dist ## 删除构建产物（保留 node_modules 与 cargo 缓存）
	cargo clean -p mcm-app -p mcm-core -p mcm-export 2>/dev/null || true

distclean: clean ## 彻底清理（含 node_modules 与整个 target/）
	rm -rf node_modules target

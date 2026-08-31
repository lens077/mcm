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

# 版本以 tauri.conf.json 为准（bump 脚本会保证它与 Cargo.toml 一致）
VERSION = $(shell node -p "require('./src-tauri/tauri.conf.json').version" 2>/dev/null)

# make release BUMP=minor / make bump BUMP=major
BUMP ?= patch

# 清洗 gh run view 的日志：剥掉 job/step/时间戳前缀，再去掉颜色转义。
# 注意 gh 输出里的转义是**字面文本** ^[[1m 而非真正的 ESC 字节，
# 两种形式都要处理，否则 grep 会漏掉带颜色的 error 行（实测踩过）。
STRIP_LOG = sed -E -e 's/^.*[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9:.]+Z //' \
                   -e 's/\x1b\[[0-9;]*m//g' \
                   -e 's/\^\[\[[0-9;]*m//g'

.PHONY: help install dev web build build-universal bundle \
        site-install site-dev site-build site-preview site-shots site-deploy \
        fmt fmt-check lint lint-rs lint-ci test test-rs test-web \
        bench smoke gate ci \
        check-bundle measure-startup fixtures \
        clean clean-dist distclean verify-clean-checkout \
        version bump-preview release releases release-watch \
        ci-status ci-watch ci-log ci-fail \
        verify-dmg verify-checksums package-purge

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

# ────────────────────────────── 宣传站点 ──────────────────────────────
#
# site/ 是独立的 Astro 项目，不在 pnpm workspace 内，也不在 release 的
# 触发白名单里——改站点不会发新版本，符合预期。

site-install: ## 安装站点依赖
	cd site && pnpm install

site-dev: ## 本地开发宣传站点
	cd site && pnpm dev

site-build: ## 构建宣传站点静态产物（site/dist）
	cd site && pnpm install --silent && pnpm build

site-preview: site-build ## 本地预览构建后的站点
	cd site && pnpm preview

# 截图是真实渲染：场景数据出自 mcm-core，再由应用真实的前端渲染器绘制。
site-shots: $(DIST_ENTRY) ## 重新生成站点产品截图
	cargo run -q -p mcm-core --example dump_scenes -- \
		site/fixtures/demo.mcm /tmp/mcm-scenes
	node scripts/capture-screens.mjs /tmp/mcm-scenes site/public/shots

site-deploy: site-build ## 部署站点到 node1
	tar -C site/dist -czf - . | ssh node1 \
		'rm -rf /home/docker/mcm-site/site/* && tar -C /home/docker/mcm-site/site -xzf - && docker restart mcm-site'
	@echo "已部署 https://mcm.apikv.com"

# ─────────────────────────────── 发布 ───────────────────────────────
#
# 正常情况下不需要手动发版：push 到 main 且改了代码，release 工作流会自动
# 递增补丁版本并发布。下面这些是给「要发 minor/major」和排查时用的。

version: ## 显示当前版本号
	@echo $(VERSION)

bump-preview: ## 预演版本递增，不改任何文件（BUMP=patch|minor|major）
	@node scripts/bump-version.mjs $(BUMP) --dry-run

# 发版 = 递增版本 + 提交 + 打标签 + 推送。
# 推送标签才会触发 release 工作流构建 macOS 与 Windows 安装包。
# 前置检查从严：发版是对外动作，出错的代价比多等一次高。
release: ## 发版（BUMP=patch|minor|major，默认 patch）
	@command -v gh >/dev/null || { echo "需要 gh CLI：brew install gh"; exit 1; }
	@git diff --quiet && git diff --cached --quiet || \
	 { echo "工作区有未提交改动，先提交或暂存"; exit 1; }
	@git fetch -q origin main && \
	 [ "$$(git rev-parse HEAD)" = "$$(git rev-parse origin/main)" ] || \
	 { echo "本地与 origin/main 不一致，先 git pull --rebase"; exit 1; }
	@new=$$(node scripts/bump-version.mjs $(BUMP)) && \
	 cargo update -w >/dev/null 2>&1 && \
	 git add Cargo.toml Cargo.lock src-tauri/tauri.conf.json && \
	 git commit -q -m "chore(release): v$$new" && \
	 git tag -a "v$$new" -m "v$$new" && \
	 git push -q origin main && \
	 git push -q origin "v$$new" && \
	 echo "已发布 v$$new —— 用 make release-watch 跟踪构建"

releases: ## 列出已发布版本
	@gh release list --limit 10

release-watch: ## 跟踪最近一次发版运行直到结束
	@id=$$(gh run list --workflow=release.yml --limit 1 --json databaseId \
	        --jq '.[0].databaseId'); \
	 echo "release run $$id"; \
	 gh run watch "$$id" --exit-status

# ───────────────────────────── CI 观测 ─────────────────────────────

ci-status: ## 列出最近的工作流运行
	@gh run list --limit 10

ci-watch: ## 跟踪最近一次 CI 运行直到结束
	@id=$$(gh run list --workflow=ci.yml --limit 1 --json databaseId \
	        --jq '.[0].databaseId'); \
	 echo "ci run $$id"; \
	 gh run watch "$$id" --exit-status

# 默认取最近一次失败的运行；也可以 make ci-log RUN=<id>
ci-log: ## 查看运行日志（RUN=<id> 指定，默认最近一次失败）
	@id="$(RUN)"; \
	 [ -n "$$id" ] || id=$$(gh run list --status failure --limit 1 \
	        --json databaseId --jq '.[0].databaseId'); \
	 [ -n "$$id" ] || { echo "没有失败的运行"; exit 0; }; \
	 gh run view "$$id" --log-failed | $(STRIP_LOG)

ci-fail: ## 只看最近一次失败运行的错误行（比 ci-log 精简）
	@id=$$(gh run list --status failure --limit 1 --json databaseId \
	        --jq '.[0].databaseId'); \
	 [ -n "$$id" ] || { echo "没有失败的运行"; exit 0; }; \
	 echo "run $$id"; \
	 gh run view "$$id" --log-failed 2>/dev/null | $(STRIP_LOG) \
	   | grep -iE "error[:[]|panicked|assertion|not found|FAILED|exit code" \
	   | head -30

# ───────────────────────────── 产物核验 ─────────────────────────────

verify-dmg: ## 校验 macOS DMG：校验和 + 能否挂载 + 内含 .app
	@dmg=$$(ls target/release/bundle/dmg/*.dmg 2>/dev/null | head -1); \
	 [ -n "$$dmg" ] || { echo "未找到 DMG，先 make bundle"; exit 1; }; \
	 echo "→ $$dmg"; \
	 hdiutil verify "$$dmg" 2>&1 | grep -iE "valid|invalid"; \
	 mp=$$(hdiutil attach "$$dmg" -nobrowse -readonly 2>/dev/null \
	       | tail -1 | awk '{print $$NF}'); \
	 [ -n "$$mp" ] || { echo "挂载失败"; exit 1; }; \
	 echo "挂载内容：$$(ls "$$mp" | tr '\n' ' ')"; \
	 hdiutil detach "$$mp" >/dev/null 2>&1; \
	 echo "✅ DMG 可用"

verify-checksums: ## 下载指定版本的 SHA256SUMS 并校验附件（TAG=v0.1.1）
	@tag="$(TAG)"; \
	 [ -n "$$tag" ] || tag="v$(VERSION)"; \
	 dir=$$(mktemp -d); \
	 echo "→ $$tag"; \
	 gh release download "$$tag" -D "$$dir" --clobber || exit 1; \
	 cd "$$dir" && shasum -a 256 -c SHA256SUMS; \
	 rm -rf "$$dir"

# 早期版本曾把安装包推到 ghcr，现已停止。这条用来清理遗留镜像。
# 需要额外授权：gh auth refresh -h github.com -s delete:packages,read:packages
package-purge: ## 删除遗留的 ghcr 容器镜像（需 delete:packages 权限）
	@gh api -X DELETE user/packages/container/mcm \
	 && echo "已删除" \
	 || echo "失败——多半是缺权限，见上方提示或到网页 Packages 页面手动删除"

# ─────────────────────────────── 清理 ───────────────────────────────

clean-dist: ## 删除前端产物
	rm -rf dist

clean: clean-dist ## 删除构建产物（保留 node_modules 与 cargo 缓存）
	cargo clean -p mcm-app -p mcm-core -p mcm-export 2>/dev/null || true

distclean: clean ## 彻底清理（含 node_modules 与整个 target/）
	rm -rf node_modules target

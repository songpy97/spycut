# 交互式版本发布脚本设计

## 目标与边界

新增 `scripts/release.sh`，让维护者从仓库根目录运行一次命令后，通过交互选择 major、minor 或 patch，自动同步前端、Tauri、Rust 与 Cargo lockfile 的版本，执行发布前门禁，提交、创建注解标签并推送，从而触发现有 GitHub Actions 的三平台预发布流程。脚本不修改 Release Action 的 prerelease 策略，不依赖 GitHub CLI，不代替 Actions 中的原生打包和安装验收。

## 方案

最简方案是依次执行改版本、提交、push 和 tag push，但中途失败会让远端分支与标签处于不同状态。完整 GitHub CLI 方案可监控 Actions，却增加额外安装、认证和 API 状态处理。采用依赖仓库既有 Git、Node、Cargo，并可通过全局 pnpm、Corepack 或 npm 固定版本回退提供 pnpm 的防护式方案：要求在 `main`，先 fetch 并拒绝落后或分叉的分支；校验四处当前版本一致；展示工作区全部改动并两次确认；运行 `cargo check`、`pnpm check` 和 `git diff --check`；最后创建 release commit 和注解标签，通过 `git push --atomic origin main refs/tags/vX.Y.Z` 同时发布。

## 失败处理与验证

版本修改前复制四个版本文件到 `mktemp` 目录。检查失败或用户在最终确认前取消时恢复脚本启动时的原文件，保留其他已有工作区改动；一旦开始暂存和提交，不自动重写 Git 状态。若提交与标签已在本地生成但原子 push 失败，下次运行识别“当前版本标签指向 HEAD 且远端不存在”，在工作区干净时允许直接续推。脚本提供无副作用的 `--calculate VERSION major|minor|patch` 和 `--help`，Vitest 用它们验证语义版本计算、非法输入、安全命令和工作流触发约束；实际测试不会执行 commit、tag 或 push。

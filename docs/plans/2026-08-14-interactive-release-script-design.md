# 交互式版本发布脚本设计

## 目标与边界

新增 `scripts/release.sh`，让维护者从仓库根目录运行一次命令后，通过交互选择 major、minor 或 patch，自动同步前端、Tauri、Rust 与 Cargo lockfile 的版本，执行发布前门禁，提交、创建注解标签并推送，从而触发现有 GitHub Actions 的三平台预发布流程。脚本不修改 Release Action 的 prerelease 策略，不依赖 GitHub CLI，不代替 Actions 中的原生打包和安装验收。

## 方案

最简方案是依次执行改版本、提交、push 和 tag push，但中途失败会让远端分支与标签处于不同状态。完整 GitHub CLI 方案可监控 Actions，却增加额外安装、认证和 API 状态处理。采用依赖仓库既有 Git、Node、Cargo，并可通过全局 pnpm、Corepack 或 npm 固定版本回退提供 pnpm 的防护式方案：要求在 `main`，先 fetch 并拒绝落后或分叉的分支；校验四处当前版本一致；展示工作区全部改动并两次确认；运行 `cargo check`、`pnpm check` 和 `git diff --check`；最后创建 release commit 和注解标签，通过 `git push --atomic origin main refs/tags/vX.Y.Z` 同时发布。

实际发布执行 `pnpm check` 时设置 `SPYCUT_RELEASE_IN_PROGRESS=1`，只跳过会在临时 Git 仓库中再次启动 commit/tag/push 的发布集成用例，避免发布流程递归模拟自身。该集成用例在普通本地测试与 CI 中不跳过，因此原子推送行为仍有独立覆盖；其他类型检查、前端测试和 Rust 门禁在发布期间全部照常运行。

脚本设置 `GIT_PAGER=cat`，确保 `git diff --check`、diff 摘要和后续 Git 输出直接写入终端，不继承用户的分页器配置。这样检查结束后必然继续到最终确认提示，不会因 `less` 显示 `END` 而看似完成、实际仍阻塞。

如果 release commit 或注解标签已在本地创建、但远端仍无同名标签，下一次运行进入续发分支而不计算新版本。脚本展示标签后的本地提交和未提交改动；经确认后对待处理改动重跑门禁并追加一个修复提交，只允许更新这个从未发布到远端的本地标签到当前 `main`，最后原子推送分支与标签。所有插值后紧邻非 ASCII 标点的 Bash 变量都使用 `${name}` 形式，避免多字节 locale 将标点误解析进变量名。

## 失败处理与验证

版本修改前复制四个版本文件到 `mktemp` 目录。检查失败或用户在最终确认前取消时恢复脚本启动时的原文件，保留其他已有工作区改动；一旦开始暂存和提交，不自动重写 Git 状态。若提交与标签已在本地生成但原子 push 失败，下次运行识别“当前版本标签指向 HEAD 且远端不存在”，在工作区干净时允许直接续推。脚本提供无副作用的 `--calculate VERSION major|minor|patch` 和 `--help`，Vitest 用它们验证语义版本计算、非法输入、安全命令和工作流触发约束；实际测试不会执行 commit、tag 或 push。

# Source Sidecar Persistence Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将每个视频的 SpyCut 设置原子保存在源视频同目录的 `<完整文件名>.spycut.json`，并在重新导入或重启恢复时优先读取。

**Architecture:** 复用现有 `ProjectV1`，让同目录 sidecar 成为项目真相源，同时保留应用数据项目副本作为最近项目启动缓存。`ProjectStore` 独占路径推导、原子 JSON I/O、sidecar 优先级和旧缓存兼容；Tauri command 继续负责指纹、工作流锁与保存失败回滚。

**Tech Stack:** Rust, serde/serde_json, tempfile, Tokio, Tauri 2, Svelte/TypeScript, Cargo tests, Vitest.

---

### Task 1: 固定 sidecar 路径和恢复规则

**Files:**
- Modify: `src-tauri/src/infrastructure/project_store.rs`

**Step 1: Write the failing tests**

增加测试，断言 `课程 '01'.mp4` 对应 `课程 '01'.mp4.spycut.json`；保存产生 sidecar 和应用数据缓存；sidecar 比旧缓存优先；sidecar 损坏、schema 过新或指纹不匹配时返回错误且字节不被覆盖。

**Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml project_store -- --nocapture`

Expected: FAIL，因为 sidecar API 和错误分支尚不存在。

**Step 3: Implement the minimal store behavior**

新增固定后缀、从 canonical source path 推导 sidecar 路径的函数、通用原子 JSON 写入函数，以及“sidecar 优先、旧应用数据缓存后备”的 `find_matching_source`。`save` 必须先写 sidecar，再尽力刷新应用数据缓存。

**Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml project_store -- --nocapture`

Expected: PASS。

### Task 2: 接入打开、启动恢复和保存失败回滚

**Files:**
- Modify: `src-tauri/src/commands/project.rs`
- Modify: `src-tauri/tests/project_roundtrip.rs`

**Step 1: Write the failing tests**

把测试项目的 source path 放入各自临时目录，断言长项目和普通项目会从 sidecar 往返；故障注入改为在目标 sidecar 路径创建目录，并继续断言内存项目和撤销历史回滚。

**Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml project_roundtrip commands::project::tests -- --nocapture`

Expected: FAIL，直到命令和 store 使用 sidecar 路径。

**Step 3: Implement recovery integration**

`open_source` 将 store 加载错误映射为项目读取错误；`get_session` 从应用数据缓存取到源路径并验证源文件后，再调用 sidecar 优先加载，最后创建会话。保存路径仍由 `persist_project` 在 blocking 线程执行。

**Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml project_roundtrip commands::project::tests -- --nocapture`

Expected: PASS。

### Task 3: 同步产品说明和仓库契约

**Files:**
- Modify: `src/App.svelte`
- Modify: `README.md`
- Modify: `docs/SpyCut-V1-开发文档.md`
- Modify: `AGENTS.md`

**Step 1: Update user-facing copy**

状态栏说明“原视频只读，设置保存在同目录 JSON”；README 说明实际文件名、自动恢复和目录不可写时的显式保存错误。

**Step 2: Update architecture documentation**

把开发文档的保存位置、打开数据流、原子保存顺序和 ADR-005 更新为 sidecar 真相源 + 应用数据最近项目缓存。按 `AGENTS.md` 自更新协议同步项目恢复数据流和持久化安全边界。

**Step 3: Check documentation diff**

Run: `git diff --check && git diff -- AGENTS.md README.md docs/SpyCut-V1-开发文档.md`

Expected: 无空白错误，描述与源码一致且不含私人路径或视频信息。

### Task 4: 完整验证

**Files:**
- Verify only

**Step 1: Format and lint Rust**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`

Expected: PASS。

**Step 2: Run Rust and frontend checks**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`

Run: `pnpm check`

Expected: PASS。

**Step 3: Final worktree audit**

Run: `git diff --check && git status --short`

Expected: 没有意外生成文件；既有用户改动保留；只新增本功能相关变更。

# SpyCut 仓库工作指南

本文件适用于整个仓库，是代理开始工作时的第一入口。它记录稳定的产品约束、代码导航、验证要求和维护方式；具体实现以源码、配置、脚本和测试为准。

## 1. 项目定位与不可破坏的约束

SpyCut 是面向长时间课程录屏的桌面剪切工具。用户只在原始时间轴上标记“需要删除”的区间，复核后导出公开版 MP4，再进入专业剪辑软件继续加工。

任何改动都必须保持以下约束：

- 原时间轴不可重排：不能移动、复制、交换或重新排序片段。
- 项目只保存删除区间；保留区间必须由删除区间的补集确定性生成。
- 源文件始终只读，禁止原地修改或覆盖。
- 项目设置的持久化真相源是视频同目录的 `<源文件完整名称>.spycut.json`；应用数据项目副本只用于最近项目启动缓存。
- 重叠和相邻删除区间需要规范化合并，区间不得越界或反向。
- 修改区间后，既有连接点复核状态必须失效。
- 精确导出采用完整解码、原时间轴筛选和统一重编码，不退回关键帧 stream copy 或边界混合编码。
- 导出先写同盘临时文件，自动验收通过后才提交最终文件；取消和失败不得污染既有目标。
- 导出期间必须锁定项目快照，禁止切换源文件或修改区间。
- V1 输入范围是 H.264/AVC 或 H.265/HEVC 的 MP4；改变支持范围属于产品决策，必须同步修改探测、UI、测试和文档。

## 2. 技术栈与目录导航

- 桌面壳：Tauri 2。
- 后端核心：Rust，工具链以 `rust-toolchain.toml` 为准。
- 前端：Svelte + TypeScript + Vite。
- 包管理：pnpm，Node 与 pnpm 版本分别以 `.node-version`、`package.json` 为准。
- 媒体处理：受监督的 FFmpeg/FFprobe sidecar；本机开发也可使用显式配置的工具路径。

```text
spycut/
├── src/                         # Svelte 前端
│   ├── App.svelte               # 页面状态与完整工作流编排
│   └── lib/
│       ├── api/tauri.ts         # Tauri command/event 和文件对话框封装
│       ├── components/          # 播放器、含音频波形的单时间轴、区间、复核、导出 UI
│       ├── player/              # HTML video 播放适配与预览/精确 Seek
│       ├── timeline/            # 时间轴坐标、视口、缩放、平移和刻度纯函数
│       ├── types/contracts.ts   # 与 Rust 序列化结果对应的 TS 契约
│       └── utils/               # 时间等纯函数
├── src-tauri/
│   ├── src/
│   │   ├── domain/              # 时间、区间、媒体、项目、导出计划
│   │   ├── application/         # 会话、历史和导出运行状态
│   │   ├── commands/            # Tauri IPC 命令边界
│   │   ├── infrastructure/      # 文件、探测、预览流、FFmpeg、验证、恢复
│   │   ├── lib.rs               # 插件、共享状态、命令注册和退出清理
│   │   └── main.rs              # 原生入口
│   ├── tests/                   # Rust 跨模块验收测试
│   ├── Cargo.toml               # Rust 依赖
│   ├── tauri.conf.json          # 通用 Tauri/CSP/窗口配置
│   └── tauri.release.conf.json  # 原生目标发布 sidecar 与许可证资源
├── scripts/                     # 环境检查、FFmpeg 准备与原生目标打包脚本
├── third-party/ffmpeg/          # FFmpeg 许可、来源与构建记录
├── docs/                        # 产品设计、实施计划、验收和发布记录
├── package.json                 # 前端命令和依赖
└── README.md                    # 用户与开发入口
```

不要手工修改 `node_modules/`、`dist/`、`src-tauri/target/` 或打包生成目录。依赖变化需要同时提交对应 lockfile；sidecar 二进制和媒体 fixture 默认不进入 Git。

## 3. 分层规则

依赖方向应保持为：

```text
Svelte UI → Tauri commands → application/domain
                         ↘ infrastructure → filesystem/FFmpeg/local HTTP
```

- `domain/` 必须保持纯业务逻辑，不依赖 Tauri、文件系统、网络或子进程。
- `application/` 管理项目会话、撤销/重做、快照与任务状态，不实现具体 I/O。
- `commands/` 是并发、权限和序列化边界：校验项目 ID、工作流锁和导出状态后再调用应用层与基础设施层。
- `infrastructure/` 承担可失败的外部操作，并把错误转换为上层可处理的结果。
- `infrastructure/diagnostics.rs` 维护同步刷新、大小轮换和隐私脱敏的本地诊断日志；日志不可用时不得阻断应用启动或剪辑。
- `App.svelte` 当前是前端工作流协调者；可复用的视图和播放器行为应下沉到 `lib/`，不要继续把纯逻辑堆入页面。
- 主界面只使用 `TimelineEditor.svelte` 作为唯一编辑时间轴；音频波形、删除区间轨道和细导航条共享同一个视口与播放头，不得重新引入第二套刻度或播放头。
- 连续 Scrub 由 `PlayerPane.svelte` 合并预览 Seek，手势完成后才执行精确 Seek；区间边界调整只在松手时提交一次命令。
- 播放、暂停和定位命令必须按最新用户意图收敛：主动暂停造成的过期 `play()` 取消不是用户可见错误，新的预览或精确 Seek 必须使旧精确 Seek 安静失效；拖动提交/取消的定位失败或被取代后不得自动恢复播放。
- 普通预览从保留内容进入删除区间时必须自动跳到区间终点；明确手动定位落入删除区间时只放行当前区间，离开后恢复自动跳过。连接点复核的专用跳转优先于普通预览跳过。
- 正常启动必须显示空白首页，不得依据应用数据最近项目缓存自动打开视频；只有用户手动选择、文件关联或启动参数明确指定源视频时才能打开，且仍按指纹优先恢复视频同目录 sidecar。
- Rust 使用 `snake_case` 字段并通过 serde 输出 `camelCase` 时，必须同步更新 `src/lib/types/contracts.ts` 和 `src/lib/api/tauri.ts`。

## 4. 核心数据流

### 打开与恢复项目

```text
选择 MP4
→ open_source
→ 规范化路径、ffprobe、格式校验、首尾指纹
→ 优先校验并读取同目录 sidecar；不存在时按指纹读取旧应用数据缓存
→ 加载匹配项目或创建 ProjectV1
→ 原子保存 sidecar，再刷新最近项目启动缓存
→ PreviewServer 发布当前源的随机令牌 URL
→ 前端设置 <video src=previewUrl>
```

正常启动不读取磁盘项目并保持空白；`get_session` 只允许返回当前进程内已经打开的会话。手动选择、文件关联或启动参数打开视频时必须重新检查源文件身份，并在验证后优先读取同目录 sidecar；不存在时才按指纹读取旧应用数据缓存。不能因为项目 JSON 或应用数据缓存存在就跳过文件存在性、metadata、指纹和媒体探测。损坏、schema 不支持或与当前源指纹不符的 sidecar 不得被缓存掩盖或自动覆盖。

### 编辑与保存

```text
UI 操作
→ 携带当前 projectId 调用命令
→ workflow gate / 导出锁校验
→ ProjectSession 规范化编辑并记录历史
→ ProjectStore 在源视频目录原子保存 sidecar
→ 尽力刷新应用数据最近项目缓存
→ 返回新的 SessionProjection
```

sidecar 保存失败必须回滚内存项目和撤销历史；应用数据缓存失败不得否定已经提交的 sidecar。迟到的旧项目请求不得修改当前项目。播放位置保存是低优先级节流写入，不能覆盖新项目状态。

### 预览

`infrastructure/preview_server.rs` 是大文件预览的传输边界：

- 只绑定 IPv4 loopback 的随机端口，不监听局域网地址。
- HTTP URL 只包含当前源对应的随机 UUID；请求不能传入文件路径。
- 每次发布源文件都轮换令牌，旧 URL 立即失效。
- 支持 GET、HEAD、OPTIONS、无 Range、单 Range、多 Range、开放 Range、suffix Range 和 416。
- 使用固定 64 KiB 缓冲区流式读取；禁止整文件读入内存或人为截短客户端请求范围。
- 保留请求头、连接数和 socket 超时限制。
- 前端禁止重新引入 Tauri `asset://`、`convertFileSrc` 或 `protocol-asset` 作为视频预览路径。
- 修改主机名、端口策略或响应头时，要同步检查 `tauri.conf.json` 的 CSP、macOS `Info.plist` ATS 和 Windows WebView2。

### 音频波形

```text
打开或恢复项目
→ macOS 自动调用；Windows 等待预览结束后由用户手动触发
→ 前端携带当前 projectId 调用 get_audio_waveform
→ audio_waveform.rs 用受监督 FFmpeg 只解码第一条音轨
→ 8 kHz 单声道 PCM 按 20 ms 聚合为 8 位峰值
→ TimelineEditor 只聚合和绘制当前视口对应的峰值
```

- 波形是源媒体的临时派生视图，不写入项目 sidecar，也不能成为编辑真相源。
- 波形分析必须流式读取、设置有限总超时，并在超时或失败后终止和回收 FFmpeg；失败不得阻断区间编辑或导出。
- 波形解码等会跨 `await` 保留的读缓冲必须使用堆分配，Tauri 波形命令应装箱大型内部 future，并用 future 大小回归测试避免 Windows release IPC 调度再次发生栈溢出。
- `get_audio_waveform` 必须在分析前后校验 `projectId`；前端也必须丢弃切换项目后迟到的结果。
- Windows 导入关键路径不得自动启动波形 IPC；预览加载完成或明确失败后才显示可用的手动生成入口。改变该隔离策略必须先完成真实 Windows 回归。
- 波形、删除区间、播放头、刻度和全片导航必须使用同一原始时间坐标；点击或拖动波形复用现有 Scrub 预览与松手精确 Seek。

### 精确导出

```text
复核删除连接点
→ 创建不可变 ExportPlan
→ 选择并真实试编码硬件/软件编码器
→ 生成 FFmpeg filter script
→ 顺序解码并统一重编码到隐藏 partial
→ 验证流数量、编码族、起始时间、时长、音画差和连接点
→ 原子提交目标文件
```

FFmpeg/FFprobe 必须以受监督子进程运行：设置超时或进度监控，取消与退出时终止并回收子进程。不得用阻塞且无限等待的命令代替现有流程。

### 本地诊断

```text
应用启动 → 创建 session.running 并追加 app_started
导入/波形/播放或前端异常 → 同步追加脱敏事件
正常退出并回收媒体子进程 → 记录 app_exit_clean 并删除运行标记
异常退出 → 下次启动记录 previous_session_unclean
```

- Tauri 只通过 `get_diagnostic_status` 暴露受控日志位置，通过 `record_frontend_diagnostic` 接受固定类型的前端错误事件；前端不得传入任意日志路径。
- `waveform_lifecycle` 只记录固定的会话、预览和请求阶段，不得包含项目 ID、源路径、文件名或预览 URL。
- 诊断日志位于应用数据目录的 `diagnostics/`，当前文件超过 5 MiB 后只轮换保留一份旧文件；每条记录必须及时刷新。
- 日志不得记录视频文件名、绝对路径、预览 URL/令牌或课程内容；外部错误文本必须限长并脱敏。
- Windows 上所有生产 FFmpeg/FFprobe 子进程必须通过 `media_command` 创建并使用 `CREATE_NO_WINDOW`，避免导入、波形或导出时弹出控制台窗口。
- panic hook 和异常运行标记只能提供尽力诊断；本机 DLL/WebView2 访问冲突仍需结合 Windows 事件查看器。

## 5. 状态、文件与安全边界

- 项目 schema 由 `domain/project.rs` 定义。schema 变更要设计迁移或明确拒绝旧版本，不能静默误读。
- 项目保存由 `infrastructure/project_store.rs` 负责；sidecar 路径只能从已规范化的源路径推导，前端和命令层不得传入或直接写任意 JSON 路径。
- 源身份校验由 `infrastructure/fingerprint.rs` 负责，保持大文件只读取首尾而非整文件哈希。
- 中断恢复只允许清理 SpyCut 严格命名且已登记的工作文件，不能接受任意路径删除请求。
- 新增日志、测试快照、文档或提交说明不得暴露课程画面、私聊内容、源文件绝对路径或预览令牌；历史记录中发现此类内容时不要继续传播，并在任务范围允许时脱敏。
- 测试真实课程视频后应正常退出应用，确认无 `spycut`、`ffmpeg`、`ffprobe` 进程和挂载镜像遗留；临时截图应移入废纸篓或安全清理。
- 不扩大 CSP、ATS、文件访问范围或回环服务暴露面，除非需求明确且有对应安全测试。

## 6. 常用命令

首次在 macOS 准备环境：

```sh
scripts/bootstrap-macos.sh
scripts/check-env.sh
```

日常开发：

```sh
pnpm install
pnpm tauri:dev
```

提交前的默认验证：

```sh
pnpm check
git diff --check
```

分项验证：

```sh
pnpm typecheck
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
```

需要真实 FFmpeg 的忽略测试，先准备或显式指定 `SPYCUT_FFMPEG_PATH` 和 `SPYCUT_FFPROBE_PATH`，然后只运行相应的 `--ignored` 测试。大文件验收测试需要显式提供 `SPYCUT_LARGE_FIXTURE`，不得在普通测试中自动生成数 GB 文件。

发布前按目标平台使用：

- `scripts/release.sh`（首选交互式版本发布入口；选择 major/minor/patch，同步版本并完成检查后原子推送 `main` 与 `v*` 标签；无全局 pnpm 时通过 Corepack 或 npm 使用固定版本）
- `scripts/prepare-ffmpeg-macos.sh`
- `scripts/prepare-ffmpeg-windows.ps1`
- `scripts/package-macos.sh`（Apple Silicon 或 Intel macOS 上原生生成并验收对应架构的 DMG/ZIP）
- `scripts/package-windows.ps1`（Windows 原生生成 NSIS；干净 Windows/CI 上必须带 `-SmokeTest` 完成静默安装与卸载验收）
- `src-tauri/tauri.release.conf.json`

实际命令、签名状态、交叉构建限制和产物校验以 `README.md`、`docs/test-reports/` 与 `docs/release/` 为准。可发布 Windows 安装器必须在 Windows 原生生成并通过安装/卸载冒烟验收；macOS/Linux 交叉编译只可用于编译检查，不能替代 Windows 打包。安装器冒烟通过也不能替代真实 Windows 10/11 + WebView2 媒体验收。
macOS 打包验收必须确认 `.app` 的 `Info.plist` 声明 `icon.icns`，且同名资源实际存在于应用包中。

GitHub Actions 在推送 `v*` 标签时于 `macos-15` ARM64、`macos-15-intel` x64 和 `windows-2022` x64 runner 并行生成包；只有三个原生目标都成功，才允许自动创建包含安装包和校验文件的预发布 GitHub Release。手动 `workflow_dispatch` 只上传 Actions 产物，不创建 Release。
交互式发布脚本会把运行时工作区的全部改动纳入 release commit；确认发布前必须复核其 `git status` 和 diff 摘要。检查失败或最终确认前取消时脚本应恢复四个版本文件，开始提交后不得自动重写 Git 历史；原子 push 失败时保留本地 release commit 和标签供后续续推，禁止 force push 或复用已发布标签。
每个平台的校验文件必须从本次构建的 bundle 目录上传，禁止用宽泛 glob 混入 `docs/release/` 中的历史版本校验文件。

所有构建、测试、FFmpeg 和容器命令必须使用有限超时或非交互批处理模式。工具返回 session id 时必须持续轮询到退出，或明确终止会话；不能在命令已经结束后仍让任务停在 Working 状态。

## 7. 按改动范围选择验证

| 改动 | 最低验证 |
|---|---|
| Svelte 组件、样式、快捷键 | `pnpm typecheck`、相关 Vitest；实际查看目标状态且避免显示私人课程内容 |
| TS/Rust IPC 契约 | 前后端类型同步、`pnpm check`、打开/恢复项目流程 |
| 区间、时间、项目领域逻辑 | Rust 单元/集成测试，覆盖边界、合并、撤销和往返持久化 |
| 项目保存、指纹、恢复 | sidecar 路径与原子替换、旧缓存迁移、损坏/schema/指纹不符保护、保存故障回滚、大文件和越权路径测试 |
| Range 预览服务 | Range 解析 + TCP 集成测试；可用时回归尾部 `moov` 的真实大文件和跨小时 seek |
| 音频波形提取或显示 | PCM 分块与峰值聚合 Rust 测试、Tauri/TS 契约同步、加载/失败/项目切换前端测试；可用时运行显式真实 FFmpeg 短媒体测试 |
| 诊断日志或 Windows 媒体进程启动 | 日志刷新/轮换/异常标记/脱敏 Rust 测试、前端上报与日志入口测试、Windows 目标 `cargo xwin check` 或 Clippy；真机确认无黑色控制台窗口 |
| FFmpeg filter、编码器、验证 | `pnpm check` 加显式 FFmpeg 媒体测试，核对逐帧映射、取消和既有目标保护 |
| CSP、ATS、Tauri 配置 | 生产 `.app` 实际启动与预览；对应平台构建检查 |
| 发布配置或 sidecar | 重建目标包，检查架构、sidecar、许可证、签名/包结构并更新校验值和验收报告；Windows 安装器还必须原生静默安装、核对文件并卸载 |

不要为通过测试而放宽产品约束。修复回归时应先增加能复现问题的最小测试，再修改实现。

## 8. 实施与交付约定

- 开始前先运行 `git status --short`，用户已有改动必须保留；不要清理或覆盖无关文件。
- 优先做最小、可验证的改动，避免顺手重构不相关模块。
- 搜索优先使用 `rg` / `rg --files`，读取范围要有界。
- 文件编辑使用补丁方式；格式化器只用于机械格式化。
- 不使用破坏性 Git 命令，不覆盖源视频和既有导出文件。
- 长任务持续提供简短进度，任何进程或挂载都要在交付前清理。
- 产品行为或架构变化先在 `docs/plans/` 留下设计；验收结论更新到相应平台报告，不把未执行的真机测试写成已通过。
- 交付时说明改动、验证、已知平台缺口和可直接使用的产物路径。

## 9. AGENTS.md 自更新协议

这里的“自更新”指代理在执行仓库变更时主动维护本文件，不是应用运行时修改文件，也不是后台定时任务。

### 9.1 必须触发更新的变化

完成实现后，只要出现以下任一变化，就必须在同一任务中更新本文件，无需用户再次提醒：

- 新增、删除、重命名顶层目录、核心模块或主要入口。
- 改变 Rust 分层职责、依赖方向或核心数据流。
- 新增或修改 Tauri command/event、共享状态或前后端契约。
- 改变产品不可变量、支持格式、预览方案、导出策略、持久化或恢复规则。
- 改变开发、测试、构建、打包、签名命令或所需工具链。
- 改变最低验证要求、平台支持状态或重要安全边界。
- 新增子目录级 `AGENTS.md`；根文件要补充导航和覆盖关系。

以下情况通常不更新：局部实现重构但职责不变、纯文案/样式微调、测试数量变化、依赖补丁版本变化、临时文件和构建产物变化。

### 9.2 每次变更后的自检算法

在最终验证前执行：

1. 查看 `git diff --name-status`，把变更文件映射到本文件第 2～7 节。
2. 对照真实源码、`package.json`、`rust-toolchain.toml`、Tauri 配置、脚本和测试，判断现有描述是否仍成立。
3. 命中触发条件时，只更新受影响段落；不要自动重写整份文件。
4. 新路径必须确认存在，新命令必须来自当前脚本或配置；禁止凭记忆补写。
5. 运行 `git diff --check`，并人工复核 `git diff -- AGENTS.md` 是否包含易过期信息或私人数据。
6. 若任务包含提交，把 `AGENTS.md` 与造成它变化的代码放在同一提交；否则保持在同一工作区变更中交付。

### 9.3 信息来源优先级

- 运行行为、目录和命令：源码、配置、脚本、测试优先。
- 产品意图和验收边界：已确认的产品文档、设计和验收报告优先。
- 本文件只做导航和工作契约，不应成为版本号、测试计数、安装包哈希等易变事实的唯一来源。

如果来源冲突，先查明真实状态并在本次任务授权范围内同步修正；不能确定时，在交付中报告差异，不要编造结论。只读审查任务不应擅自修改本文件，但必须指出发现的漂移。

### 9.4 防止自更新失控

- 不编写运行时或 Git hook 自动覆盖 `AGENTS.md`。
- 不记录本机绝对路径、随机端口、预览令牌、用户视频名称、构建时间、测试数量或产物哈希。
- 不复制整份 README、开发文档或验收报告；只保留代理完成工作所需的稳定规则和链接。
- 子目录出现独立复杂度时可新增更近的 `AGENTS.md`；最近文件优先，但不得放宽根目录的产品、安全和数据保护约束。
- 高层系统/用户指令始终优先于本文件；发现冲突时停止采用冲突条目并说明原因。

## 10. 交付前清单

- [ ] 产品不可变量未被破坏。
- [ ] 前后端契约和序列化字段保持同步。
- [ ] 按改动范围完成测试，未把跳过项写成通过。
- [ ] `pnpm check` 或无法执行的具体原因已记录。
- [ ] 无遗留应用、FFmpeg、容器、执行会话或磁盘镜像。
- [ ] 工作树中无被覆盖的用户改动和意外生成文件。
- [ ] 本次变化命中第 9.1 节时，`AGENTS.md` 已同步更新。
- [ ] README、设计、验收报告和校验值在需要时已同步。

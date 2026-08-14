# SpyCut V1 Windows 交叉构建记录

日期：2026-08-13

目标：Windows x64 (`x86_64-pc-windows-msvc`)

构建主机：macOS 15.3.2 arm64

## 已撤回的交叉构建产物

| 文件 | 大小 | SHA-256 |
|---|---:|---|
| `SpyCut_0.1.0_x64-setup.exe` | 242,452,399 bytes | `baa22430d22fa0d017de6ff8e09401c99bae7f39859abd96c2608b1047e90dd1` |

该产物由 macOS 脚本回退到 Linux/amd64 NSIS 3.05-2 容器生成；真实 Windows 启动时出现 `Installer integrity check has failed`，已撤回，不得继续分发，也不得用 `/NCRC` 绕过安装。

## 交叉编译阶段曾验证

- `cargo xwin clippy --target x86_64-pc-windows-msvc --all-targets -- -D warnings`：通过。
- 新增的 `std::net` 本机 Range 服务已包含在上述 Windows 全目标 Clippy 和交叉链接中；仍需在 WebView2 真机验证 Range 请求行为。
- Tauri 使用 `cargo-xwin` 生成 `spycut.exe`：`PE32+ executable (GUI) x86-64, for MS Windows`。
- Tauri 2.11.4 生成当前 NSIS 脚本；macOS arm64 原生 `makensis` 在处理约 220 MiB sidecar 时内存分配失败，因此改用本机已有的 Linux/amd64 NSIS 3.05 容器，并挂载本机 NSIS 3.12 的新版 `Win/COM.nsh` 以满足当前 Tauri 模板，最终无网络打包成功。
- 安装器识别为 `PE32 executable (GUI)` 与 `Nullsoft Installer self-extracting archive`；当前主程序识别为 `PE32+ executable (GUI) x86-64`。
- NSIS 生成记录确认安装数据写入当前 `spycut.exe`、`ffmpeg.exe`、`ffprobe.exe`、`FFmpeg-NOTICE.md` 和 LGPL 许可文件。
- Windows FFmpeg 为固定版本/哈希的 BtbN LGPL 构建。

上述静态检查没有真正执行 NSIS 自校验、安装和卸载，因而未能发现坏包。以后不得再把这些检查表述为安装包验收通过。

## 新的 Windows 原生发布门禁

- macOS/Linux 只保留 `cargo xwin check/clippy` 等编译验证，不再生成可发布 NSIS。
- Windows 本地通过 `scripts/package-windows.ps1` 构建；GitHub `package` 工作流在 `windows-2022` 原生构建。
- CI 必须带 `-SmokeTest`，在无既有 SpyCut 安装的干净机器上执行安装器 `/S`，核对 `spycut.exe`、FFmpeg/FFprobe、许可证和卸载器，再静默卸载。
- 只有安装、内容核对和卸载全部成功后，`setup.exe` 与同目录 `.sha256` 才能上传。

## 0.1.2 Windows 原生发布结果

[GitHub Actions 运行 31711811297](https://github.com/songpy97/spycut/actions/runs/31711811297) 已在 `windows-2022` x64 runner 上完成发布构建。`scripts/package-windows.ps1 -SkipChecks -SmokeTest` 成功执行 NSIS 静默安装、必需文件核对和静默卸载，随后发布以下产物：

| 文件 | 大小 | SHA-256 |
|---|---:|---|
| `SpyCut_0.1.2_x64-setup.exe` | 75,443,121 bytes | `a1e341aca7833a70977db82564b28aba88fee73ea329bd468ad2db8741f90281` |

该结果证明本次原生安装包可以通过安装器自校验，并具备预期安装内容和卸载路径；它不替代真实 Windows 10/11、WebView2、显卡驱动及用户视频的媒体回归。

## 必须在真实 Windows 上继续的验收

- Windows 10 和/或 Windows 11 x64 安装、卸载、快捷方式与 WebView2 引导。
- H.264/H.265 预览，包括没有 HEVC 扩展时的提示。
- NVENC/QSV/AMF/Media Foundation 候选编码器在真实显卡/驱动上的试探与回退。
- 中文、空格、引号和长路径下的项目保存、取消、恢复和导出。
- 导出文件在目标专业剪辑软件中导入。

## 2026-08-13 闪退诊断改进（源码验证）

针对加入音频波形后“导入时先出现黑色控制台窗口、随后闪退”的反馈，源码已把导入、波形、播放诊断、编码器探测、导出和验收所用的生产 FFmpeg/FFprobe 统一改为 Windows `CREATE_NO_WINDOW` 启动，并增加本地轮换诊断日志、异常运行标记、Rust panic、前端未捕获错误以及导入/波形阶段记录。`cargo xwin check --target x86_64-pc-windows-msvc` 与 Windows 目标全 targets Clippy（`-D warnings`）均已通过。

这次源码修改尚不能证明原闪退已经在 Windows 真机消失，也不能替代 Windows 事件查看器中的异常模块/异常码。下一版安装包重建后必须在真实 Windows 10/11 上确认：导入时不再出现黑色窗口、H.264/H.265 波形均可生成、强制结束后下次启动记录 `previous_session_unclean`、界面“诊断日志”能定位 `spycut.log`，并复核日志不包含源文件路径或预览令牌。

用户提供的 0.1.1 日志显示应用可完成 `app_started` 与 `frontend_ready`，随后在自动恢复最近项目期间被异常终止，没有 Rust panic、前端异常或 `waveform_started`。将 `%APPDATA%\com.spycut.desktop\projects` 重命名后，应用可稳定进入空白首页，确认最近项目自动恢复是崩溃触发条件；对应时间的 Windows 应用程序日志没有标准 APPCRASH 事件，因此尚不能确认具体原生故障模块。

0.1.2 源码已取消普通启动时的磁盘最近项目恢复：没有显式启动文件时直接显示空白首页，`get_session` 只返回当前进程内会话；手动选择或显式打开视频仍执行完整探测、指纹和 sidecar/兼容缓存恢复。`open_source` 同时补充了 FFprobe 定位、媒体探测、指纹、项目读取、保存、预览发布和会话投影的失败阶段日志。该版本的 Windows 原生 CI 安装包与安装/卸载冒烟已通过，真机媒体复测仍待完成。

旧交叉构建安装器已明确失败并撤回；0.1.2 已改由 Windows 原生流水线生成并通过安装包冒烟。原闪退是否在真实 Windows 10/11 上完全消失，仍以新版本的真机导入、预览和波形验证为准。

## 0.1.3 Windows 波形启动隔离

0.1.2 真机日志进一步确认：首次导入在 `resumed=false` 时仍异常退出，且 `source_open_completed` 已写入而 `waveform_started` 尚未出现。播放器、HTML 视频适配器和 Range 预览服务相对波形功能引入前没有变化，因此 0.1.3 将回归边界收窄到导入后同时挂载界面、加载预览与自动发起波形 IPC 的前端时序。

0.1.3 在 Windows 上取消导入关键路径中的自动波形请求：先等待预览加载完成或明确失败，再由用户点击“生成音频波形”。macOS 保持自动生成。新增的 `waveform_lifecycle` 日志会记录 `session_applied`、`preview_loaded`、`request_prepared` 与 `request_dispatched`，且不包含项目 ID、源路径、文件名或预览 URL。

[GitHub Actions 运行 31716962768](https://github.com/songpy97/spycut/actions/runs/31716962768) 已在 `windows-2022` x64 runner 上完成完整检查、原生 NSIS 构建、静默安装、必需文件核对和静默卸载，并发布 `SpyCut_0.1.3_x64-setup.exe`（75,416,616 bytes，SHA-256 `a1d365b10452826cfd6db47b8badf093cf47bdebdbf2b5eab49bcb37113560e6`）。真实 Windows 视频导入和点击生成波形仍需用户机器复测，不能仅凭安装包冒烟写成闪退已解决。

## 0.1.4 波形按钮命中修复

0.1.3 已确认 Windows 可以顺利导入原问题视频，但波形容器的 `pointer-events: none` 使“生成音频波形”按钮事件穿透到时间轴，用户点击只会移动播放头。0.1.4 保留波形轨道空白区域的 Scrub 穿透，只为按钮设置独立命中和更高层级，并用静态 CSS 回归测试锁定该规则。发布后仍需在真实 Windows 上确认按钮可以点击并进入 `request_prepared`、`request_dispatched` 与 `waveform_started` 阶段。

[GitHub Actions 运行 31719158757](https://github.com/songpy97/spycut/actions/runs/31719158757) 已在 `windows-2022` x64 runner 上完成完整检查、原生 NSIS 构建、静默安装、必需文件核对和静默卸载，并发布 `SpyCut_0.1.4_x64-setup.exe`（75,425,986 bytes，SHA-256 `0b125b9e86cfb2114d0a884605faa9bc7d7e07e3808658621fd877306f4ad832`）。安装包门禁已通过；按钮命中与实际波形生成仍需在用户的真实 Windows 环境复测。

## 0.1.4 真机波形 IPC 闪退诊断与待发布修复

用户提供的完整日志包含两次 0.1.4 手动波形请求：一次为约三小时的 H.265 输入，一次为约 114 秒的 H.264 输入。两次都完成导入、预览和 `request_prepared`、`request_dispatched`，随后进程异常终止；均没有 `waveform_command_entered`（该事件为修复后新增）、`waveform_started`、`waveform_failed`、`waveform_completed`、`rust_panic` 或正常退出记录。下一次启动的 `previous_session_unclean=true` 进一步确认这不是可恢复的 FFmpeg 错误。短 H.264 与长 H.265 的相同结果也排除了视频时长、视频编码族和大波形返回值是主要触发条件。

编译器类型大小检查测得，原实现中的 `extract_audio_waveform` future 为 66,032 字节，`get_audio_waveform` future 为 66,192 字节，Tauri IPC 包装后为 132,392 字节。根因是 64 KiB 固定读取数组跨 `await` 被保存在异步状态机中，release 版 Tauri 又按值包装该 future，具有在 Windows 默认较小线程栈上于命令首条日志执行前溢出的条件。

待发布源码已将 PCM 读取缓冲改为等长堆分配，将提取 future 在 Tauri command 边界显式装箱，并在任何会话读取前同步记录 `waveform_command_entered stage=session_validation`。修改后对应 future 大小降为 520、288 和 584 字节；新增稳定回归测试要求提取 future 不超过 4 KiB。本机聚焦波形测试、显式真实 FFmpeg 短媒体测试、`pnpm check` 和 `cargo xwin check --target x86_64-pc-windows-msvc` 均已通过。

以上结果证明源码已消除已识别的超大 future，但不能写成 Windows 真机闪退已经通过。下一版 Windows 原生安装包仍需在原机器复测长 H.265 与短 H.264：点击后应至少记录 `waveform_command_entered` 和 `waveform_started`，最终记录 `waveform_completed` 或在界面显示可恢复的 `waveform_failed`，且不得产生新的 `previous_session_unclean`。

## 0.1.5 Windows 原生发布结果

[GitHub Actions 运行 31760281256](https://github.com/songpy97/spycut/actions/runs/31760281256) 已在 `windows-2022` x64 runner 上完成完整检查、原生 NSIS 构建、静默安装、必需文件核对和静默卸载，并发布 `SpyCut_0.1.5_x64-setup.exe`（75,439,659 bytes，SHA-256 `96a1ae9f936f091090817cef3f51f777000885d84b80fa763e6c2eeae5fe5944`）。macOS 与 Windows 原生产物全部成功后，流水线已创建 [SpyCut v0.1.5 预发布版](https://github.com/songpy97/spycut/releases/tag/v0.1.5)。

该结果证明安装包完整性、安装内容和卸载路径通过自动门禁，并包含缩小后的波形 future；它仍不替代原问题机器上的真实 H.264/H.265 波形生成复测。

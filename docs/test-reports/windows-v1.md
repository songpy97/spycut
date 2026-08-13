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

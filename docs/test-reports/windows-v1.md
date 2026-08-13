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
- 只有安装、内容核对和卸载全部成功后，`setup.exe` 与同目录 `.sha256` 才能上传。正式 Windows 原生产物的大小和 SHA-256 只能在该工作流实际通过后补录，当前不得编造。

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

0.1.2 源码已取消普通启动时的磁盘最近项目恢复：没有显式启动文件时直接显示空白首页，`get_session` 只返回当前进程内会话；手动选择或显式打开视频仍执行完整探测、指纹和 sidecar/兼容缓存恢复。`open_source` 同时补充了 FFprobe 定位、媒体探测、指纹、项目读取、保存、预览发布和会话投影的失败阶段日志。该版本仍须等待 Windows 原生 CI 安装包和真机媒体复测。

本报告目前只证明 Windows 代码和链接检查成功；交叉构建安装器已明确失败并撤回。新的 Windows 原生安装包尚未在本工作区生成，不能写成已通过。

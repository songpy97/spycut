# SpyCut V1

[![CI](https://github.com/songpy97/spycut/actions/workflows/ci.yml/badge.svg)](https://github.com/songpy97/spycut/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

[下载预览版](https://github.com/songpy97/spycut/releases) · [报告问题](https://github.com/songpy97/spycut/issues) · [参与贡献](CONTRIBUTING.md)

> 当前 GitHub Release 是未签名、未公证的开源预览版。Windows SmartScreen 或 macOS Gatekeeper 可能显示发布者警告；请先核对随产物提供的 SHA-256。正式公开发行仍需要 Apple Developer ID、Windows 代码签名和对应平台真机验收。

SpyCut 是一个只做一件事的桌面工具：在长录屏的原始时间轴上标记需要删除的区间，逐处复核后精确导出公开版 MP4。它不允许移动、复制或重排片段，因此不会出现传统剪辑软件中误拖片段、顺序错乱的问题。

## 当前成品

- macOS Apple Silicon 安装包：GitHub Release 中的 `SpyCut_*_aarch64.dmg`
- macOS Apple Silicon 免安装压缩包：GitHub Release 中的 `SpyCut_*_aarch64.zip`
- Windows x64 NSIS 安装包：只从 Windows 原生 `package` 工作流或 Windows PowerShell 构建产物获取；macOS 交叉构建旧包已撤回
- macOS 校验值由 Release 中的 `SpyCut_*_checksums.txt` 提供；Windows 原生包使用相邻的 `.sha256`

当前 macOS 包使用隔离构建的 FFmpeg 8.0.1 sidecar，不依赖 Homebrew；它只启用了 LGPL 配置和 Apple VideoToolbox。Windows 包内置固定 SHA-256 的 BtbN LGPL FFmpeg。macOS 包采用本地临时签名，Windows 包未签名；两者都尚未做正式签名或公证。

预览不再通过 Tauri 默认资源协议直接读取视频，而是由 Rust 在本机回环地址提供标准 HTTP Range 流。服务只发布当前源文件的随机令牌 URL，以 64 KiB 固定缓冲区按请求范围读取，因此数 GB 文件、位于文件尾部的 MP4 `moov` 和跨小时随机跳转都不需要整文件复制或载入内存。

## 使用方法

1. 打开 MP4 录屏。V1 接受 H.264/AVC 与 H.265/HEVC 的 MP4。
2. 打开后程序会在后台分析第一条音轨，并在时间刻度与删除区间之间显示讲话波形；通过低振幅的停顿和句尾定位，再单击或拖动波形/播放头到不应公开内容的起点。点击“设删除起点”，也可按 `I`。波形生成失败不影响剪切和导出。
3. 继续拖动或播放到终点，点击“完成删除区间”，也可按 `O`。红色区间表示将从成片中删除的内容。
4. 选中红色区间后拖动两端手柄；时间会吸附到视频帧。聚焦手柄后也可用方向键逐帧调整。区间主体只能用于选择和定位，不能被移动。
5. 长录屏可用 `Alt/Option + 滚轮` 以鼠标位置为中心连续缩放时间轴；触控板横向滑动或 `Shift + 滚轮` 可平移，底部细导航条用于跨小时快速定位，“适配全片”可恢复全局视图。
6. 点击“检查并导出”，逐个试听连接点。试听会播放删除点前 3 秒，跳过红色区间，再播放后 3 秒。
7. 选择目标 MP4。程序先写隐藏的临时文件，完成编码和自动验收后才提交最终文件。

源视频始终只读。项目设置会原子保存在视频同目录的 `<视频完整文件名>.spycut.json`，例如 `课程 01.mp4.spycut.json`；删除区间、播放位置和连接点复核状态会在再次手动选择该视频时恢复。SpyCut 正常启动时始终显示空白首页，不会自动加载上次视频；由文件关联或启动参数明确传入的视频仍会直接打开。系统应用数据目录中的 `projects/` 只保留旧缓存迁移副本，不控制启动页面；若视频旁没有 sidecar，手动选择视频时仍可按指纹从兼容缓存恢复。若视频目录不可写，应用会明确提示保存失败，不会假装编辑已经保存。异常中断留下的 SpyCut 导出临时文件会在下次启动时列出，用户可定位或安全清理。

若遇到闪退、波形生成失败或预览异常，可点击首页右下角的“打开诊断日志”，或编辑页标题栏的“诊断日志”，在系统文件管理器中定位 `spycut.log`。Windows 路径为 `%APPDATA%\com.spycut.desktop\diagnostics\spycut.log`，最近项目兼容缓存位于相邻的 `projects\`；日志只保留当前文件和一份轮换文件，记录启动、正常/异常退出标记、媒体处理阶段、Rust panic 和前端未捕获错误，不记录源视频文件名、绝对路径、预览 URL、令牌或课程内容。Windows 安装包自带 FFmpeg/FFprobe，用户不需要另外安装；媒体子进程以无控制台窗口方式启动。

复核页会提示 VFR、多视频/音频流、非 AAC 音频和 10-bit 输入。未逐处复核的连接点以及 10-bit 转 8-bit 都必须再次明确确认，后端不会接受绕过确认的默认导出请求。导出期间区间编辑和源文件切换会被锁定。

## 常用快捷键

| 快捷键 | 功能 |
|---|---|
| `Space` | 播放 / 暂停 |
| `I` / `[` | 标记删除起点 |
| `O` / `]` | 标记删除终点 |
| `←` / `→` | 后退 / 前进 1 秒 |
| `Shift` + `←` / `→` | 后退 / 前进 5 秒 |
| `Cmd/Ctrl` + `←` / `→` | 后退 / 前进 30 秒 |
| `J` / `K` / `L` | 降速 / 暂停 / 加速 |
| `Cmd/Ctrl` + `Z` | 撤销 |
| `Cmd/Ctrl` + `Shift` + `Z` | 重做 |
| `Delete` | 删除当前选中的删除区间 |
| `Esc` | 取消未完成标记或关闭复核页 |

时间轴中的播放头和区间边界获得焦点后，方向键按帧移动，`Shift + 方向键` 按 1 秒移动；该局部操作优先于上表中的全局跳转快捷键。

## 精确导出的含义

V1 不在关键帧处做无损截断，也不采用边界附近“半复制半重编码”。FFmpeg 顺序解码整个源视频，通过原始时间表达式筛选保留帧，再将全部保留内容统一重编码。这避免了关键帧偏移、片段参数不一致和顺序混乱。

- 视频边界：吸附到源帧，目标误差不超过一帧。
- 音频边界：切成 32 个采样的小块后筛选，48 kHz 时理论边界粒度约 0.67 ms。
- 输出编码族：H.264 输入仍输出 H.264；H.265 输入仍输出 H.265。
- 输出元数据和章节：默认移除，避免把录屏中的私有元数据带入公开版。
- 输出提交：先写同盘隐藏 partial，完成流数量、起始时间、时长、音画差和连接点解码验收后才提交最终文件。
- Main10：V1 会明确提示并在二次确认后输出兼容性更广的 8-bit，而不是静默改变位深。

## 开发

```sh
scripts/check-env.sh
pnpm install
pnpm check
pnpm tauri dev
```

准备 macOS LGPL sidecar 并生成发布包：

```sh
scripts/prepare-ffmpeg-macos.sh
pnpm tauri build --config src-tauri/tauri.release.conf.json --bundles app
```

Windows PowerShell（正式 Windows 安装包的唯一受支持本地构建方式）：

```powershell
./scripts/package-windows.ps1
```

在没有既有 SpyCut 安装的干净 Windows 虚拟机上，可增加安装/卸载冒烟验收：

```powershell
./scripts/package-windows.ps1 -SmokeTest
```

在 Apple Silicon macOS 上生成并验证 macOS DMG/ZIP：

```sh
bash scripts/package-macos.sh
```

GitHub Actions 的 `package` 工作流可手动运行，也会在推送 `v*` 标签时自动运行。它在 `windows-2022` 上原生生成 NSIS，随后静默安装到隔离目录，核对主程序、FFmpeg/FFprobe、许可证和卸载器，再完成静默卸载；只有全部通过才上传 `setup.exe` 和相邻的 `.sha256`。标签构建会在 macOS 和 Windows 产物都通过后自动创建 GitHub Release。Tauri 官方说明 macOS/Linux 上的 Windows NSIS 交叉构建限制更多、测试较少，应仅作为 Windows 虚拟机和 CI 都不可用时的最后手段，因此 SpyCut 不再把交叉构建的 NSIS 当作可发布安装器。

Windows 原生工作流完成的是安装器完整性、安装内容和卸载冒烟验收；仍需在真实 Windows 10/11 x64 机器上完成 WebView2、H.264/H.265 预览、波形和导出验收。

完整架构与边界条件见 [SpyCut V1 开发文档](docs/SpyCut-V1-开发文档.md)，实施记录见 [V1 验收报告](docs/SpyCut-V1-验收报告.md)，平台明细见 [macOS 验收记录](docs/test-reports/macos-v1.md) 与 [Windows 交叉构建记录](docs/test-reports/windows-v1.md)。

## 发布注意

FFmpeg 的构建与许可证记录位于 `third-party/ffmpeg/`。公开分发前仍需：

- 使用 Apple Developer ID / Windows 代码签名证书签名；
- 对安装包做 macOS 公证和真实 Windows x64 验收；
- 在下载页同时提供对应 FFmpeg 源码与构建信息；
- 根据分发地区审查 H.264/H.265 专利义务。

这些事项不影响源码开放和预览版测试，但在完成审查前不应把安装包作为正式商业发行版分发。

## 开源许可

SpyCut 源代码采用 [MIT License](LICENSE)。随安装包分发的 FFmpeg 及其许可证、来源和构建记录见 [`third-party/ffmpeg/`](third-party/ffmpeg/)；FFmpeg 不因本项目的 MIT License 而改变其自身许可。

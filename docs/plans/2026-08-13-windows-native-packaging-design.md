# Windows 原生安装包发布设计

## 背景与选择

最新的 macOS 一键打包脚本在本机 NSIS 3.12 因大型 FFmpeg sidecar 内存不足后，回退到 Linux/amd64 NSIS 3.05-2 容器。该安装器虽能在 macOS 上通过文件类型、长度、SHA-256 和静态 CRC 计算，却在真实 Windows 上被 NSIS 自校验拒绝。现有流程的问题不只是某个参数，而是把“交叉编译成功”误当成“Windows 安装器可发布”。Tauri 官方也明确说明：Windows 安装器应优先在 Windows 电脑构建，macOS/Linux 的 NSIS 交叉构建有额外限制，建议只在 Windows 虚拟机或 CI 不可用时作为最后手段。

可选方案有三种：继续修补 Linux NSIS 容器、改发免安装 ZIP、或在 Windows 原生构建。继续修补仍无法在生成主机上执行安装器，不采用；免安装 ZIP 可作为应急备份，但失去快捷方式、卸载和 WebView2 引导，不替代正式安装器；本次选择 Windows 原生 NSIS。macOS 只生成 DMG/ZIP，不再生成或发布 Windows `setup.exe`。Windows 安装器由 Windows PowerShell 脚本或 GitHub Actions `windows-2022` 运行器生成。

## 发布门禁

Windows 脚本准备固定哈希的 LGPL FFmpeg/FFprobe，运行测试后调用 Tauri 原生 NSIS bundler。构建前删除同名旧产物，防止失败后误上传陈旧文件。构建完成后写同目录 SHA-256 伴随文件。

CI 的 Windows 打包任务必须在干净机器上执行安装器 `/S`，将程序安装到专用临时目录，核对 `spycut.exe`、`ffmpeg.exe`、`ffprobe.exe`、许可证和卸载器，再静默卸载并确认主文件被删除。安装器 CRC、自解压或权限流程任一失败都会使任务失败，产物不会上传。为避免损坏开发者电脑上既有 SpyCut 安装，冒烟安装只通过显式 `-SmokeTest` 开启，并在发现既有卸载注册项时拒绝运行。

GitHub Actions 上传 `setup.exe` 与 `.sha256` 为同一个 Windows artifact。正式分发仍需 Windows 代码签名；未签名状态和真机 H.264/H.265 剪辑验收继续作为已知缺口记录。

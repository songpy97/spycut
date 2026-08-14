# SpyCut V1 macOS 验收记录

> 2026-08-14 0.1.5 发布补充：[GitHub Actions 运行 31760281256](https://github.com/songpy97/spycut/actions/runs/31760281256) 已在 `macos-15` ARM64 runner 上完成完整检查和原生打包。发布产物为 `SpyCut_0.1.5_aarch64.dmg`（26,151,049 bytes，SHA-256 `6085966dc6af83ecb4ee5dbc206e5ce321e1bd0ecac56fe915d9b319ccbe6c62`）及 `SpyCut_0.1.5_aarch64.zip`（23,706,515 bytes，SHA-256 `986c6b778391ce8a80c5f95b03c5a2f270a0989d3f7e9108e0ff8ad1d999ce86`）。本次将波形解码 future 的固定缓冲改为堆分配并在 Tauri 命令边界装箱，不改变 macOS 自动生成波形的产品行为。

> 2026-08-14 0.1.4 发布补充：[GitHub Actions 运行 31719158757](https://github.com/songpy97/spycut/actions/runs/31719158757) 已在 `macos-15` ARM64 runner 上完成完整检查和原生打包。发布产物为 `SpyCut_0.1.4_aarch64.dmg`（26,153,184 bytes，SHA-256 `8b5d8577fa2c47523c0fc152035db4901df6197200a7256f07030d970ab4a63a`）及 `SpyCut_0.1.4_aarch64.zip`（23,708,322 bytes，SHA-256 `0dc1ad3e2b89794c1069f1afdfa4195e5e8f51ad394e42a06370bfedd40d9cc6`）。本次仅修复手动波形按钮的点击命中，不改变 macOS 自动生成波形的行为。

> 2026-08-13 0.1.3 发布补充：[GitHub Actions 运行 31716962768](https://github.com/songpy97/spycut/actions/runs/31716962768) 已在 `macos-15` ARM64 runner 上完成完整检查和原生打包。发布产物为 `SpyCut_0.1.3_aarch64.dmg`（26,153,971 bytes，SHA-256 `fd23c1866e3ea41f2bf417815d6474414bd198f95547e38f327b9e139b4478d0`）及 `SpyCut_0.1.3_aarch64.zip`（23,708,551 bytes，SHA-256 `60d68e6f328aa4c4c5db0c0d426dbf8a9a64c94592263573bfe42ec8f191c2f9`）。本次 Windows 波形启动隔离不改变 macOS 自动生成波形的行为。

> 2026-08-13 发布检查补充：`scripts/package-macos.sh` 的 DMG、ZIP 和校验文件名已改为从 `tauri.conf.json` 读取当前版本，避免升级应用版本后附件仍沿用旧版本号。[GitHub Actions 运行 31711811297](https://github.com/songpy97/spycut/actions/runs/31711811297) 已用修正后的脚本在 `macos-15` ARM64 runner 上重新生成并发布 0.1.2 产物。

## 0.1.2 原生发布包

| 文件 | 大小 | SHA-256 |
|---|---:|---|
| `SpyCut_0.1.2_aarch64.dmg` | 26,153,096 bytes | `55865e71b00c0721bc791c1eb9f1afeaa2104aed4644fe477aa70315ce668194` |
| `SpyCut_0.1.2_aarch64.zip` | 23,708,006 bytes | `b457d69f49c429fdded997bbd8b8a3bac401e4effdad3a73831cb81e10452341` |

原生打包脚本已完成架构、sidecar、许可、签名、DMG 和 ZIP 完整性检查；本次 CI 结果不表述为一轮新的人工界面回归。

日期：2026-08-13

系统：macOS 15.3.2，arm64

应用：SpyCut 0.1.0

## 0.1.0 发布包

| 文件 | 大小 | SHA-256 |
|---|---:|---|
| `SpyCut_0.1.0_aarch64.dmg` | 26,142,428 bytes | `12d6c4ca3b773a8f9f2335897fc3e5f45d0eac16e9c74e37ebf23bbb4e1e296a` |
| `SpyCut_0.1.0_aarch64.zip` | 23,703,279 bytes | `eb32cf625e54b2ef6882b41ce5155249409a32f06e2e902b953987898e010c27` |

应用内置 arm64 LGPL FFmpeg/FFprobe sidecar 及 `FFmpeg-NOTICE.md` / LGPL 许可文件，不依赖本机 Homebrew FFmpeg。

## 执行结果

- 2026-08-13 增量重建执行 `pnpm check`：Svelte 0 errors/0 warnings，Vitest 46 passed，Rust 默认测试 59 passed/5 ignored，Clippy `-D warnings` 和 rustfmt 通过；另以打包的 FFmpeg/FFprobe 执行音频波形 MP4 集成测试，通过。
- 单时间轴在 960×680 和 1440×920 本地页面完成布局与交互验收：仅一套标尺/播放头，连续 Scrub、区间边界、起止按钮、缩放、细导航条和导出锁定工作正常；播放器、删除区间面板、时间轴和状态栏随窗口收缩且无页面溢出。这不替代 Tauri 触控板真机回归。
- 新 `.app` 重新执行 ad-hoc 深层签名；DMG 挂载后再次验证主程序、FFmpeg/FFprobe 均为 arm64，许可文件存在，签名满足 Designated Requirement。
- 2026-08-10 显式执行的 3 个 FFmpeg 重型媒体测试为本次重建的媒体基线，本次仅修改前端时间轴，未重复该组重型测试。基线覆盖 H.264/H.265、29.97/30/60 fps、有声/无声、Main10 转 8-bit、逐帧映射和取消清理。
- 新增本机 HTTP Range 预览服务的解析和 TCP 集成测试：覆盖无 Range、HEAD、普通/开放/suffix/多 Range、416、OPTIONS/405、令牌轮换，以及完整返回 1,500,001 字节范围。
- 合成 3 小时、3840×2160、H.265 + AAC、大于 11 GiB 的完整 MP4：FFprobe 约 0.06 s，Rust 大文件 probe + 首尾指纹验收约 0.10 s，源文件长度不变。该临时样片验收后已删除以回收约 11 GiB 磁盘。
- 最终签名 `.app` 成功打开一个逻辑大小超过 11 GiB 的稀疏 4K H.265 + AAC MP4，探测到 3840×2160、HEVC、单视频流和单音频流，没有复制整个源文件。
- 使用本地私有的多 GB、数小时、5K HEVC VFR 测试样本回归；`moov` 位于文件尾部。源文件未转码、未重封装，WKWebView 达到 `HaveMetadata` 和 `HaveEnoughData` 并显示画面；跨小时 seek 在亚秒级完成，进程 RSS 保持约百 MB，没有随源文件大小增长。样本路径和课程内容未写入公开记录。
- 两个通过 SpyCut 自动验收的 H.264/H.265 MP4 均由 QuickTime Player 实际打开，返回时长均为 3.734 s。
- 最终 `.app` 实际启动并通过正常 Quit 路径退出；退出后无 `spycut`、`ffmpeg` 或 `ffprobe` 进程遗留。
- `hdiutil verify` 返回 VALID；只读挂载 DMG 后，镜像内 `.app` 通过 `codesign --verify --deep --strict`，FFmpeg/FFprobe 可执行且许可文件存在。
- ZIP 通过 `unzip -t`。

## 签名边界

`.app` 使用 ad-hoc 本地签名，没有 Apple TeamIdentifier，DMG 未公证。这足以做本机内部测试，不等同于面向公众的 Developer ID 签名与 notarization。

## 未覆盖

- 用户真实课程录屏的完整内容和完整导出验收（真实文件预览与随机跳转已覆盖）。
- Final Cut Pro、Premiere Pro、DaVinci Resolve 或剪映/剪映专业版导入；当前机器上未安装这些软件。
- Intel Mac。

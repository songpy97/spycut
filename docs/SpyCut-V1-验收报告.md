# SpyCut V1 验收报告

日期：2026-08-12

应用：SpyCut 0.1.0

本地验收环境：macOS 15.3.2，Apple Silicon

## 结论

SpyCut V1 已实现“只标记删除区间、不能重排片段、逐处复核、精确重建导出”的完整工作流。Apple Silicon Mac 安装包已在本机完成启动、媒体、大文件、精确导出、播放器导入、退出清理和包完整性验收。Windows x64 主程序可完成交叉编译检查；此前由 macOS/Linux NSIS 交叉打包的安装器已在真实 Windows 上出现完整性错误并撤回，必须由 Windows 原生重新生成和验收。

## 产品与安全边界

- 原视频时间轴不可变，项目只保存删除区间，不存在移动、复制或重排片段的命令。
- 删除区间可新增、拖动边界、删除、合并、撤销/重做；保留段由补集确定性生成。
- 项目 JSON 原子写入，源文件用大小、时间和首尾 BLAKE3 指纹校验；重启后自动恢复最近项目。
- 导出中禁止切换源文件或修改区间；前端请求和后端快照都校验项目编号，避免迟到请求污染新项目。
- 未复核连接点必须第二次明确确认，后端也会拒绝绕过 UI 的默认导出。
- Main10/10-bit 源会明示警告 V1 输出为 8-bit，必须再次确认。VFR、多音视频流、非 AAC 音频也会在复核页告警。
- 导出先写同盘隐藏 partial，自动验收通过后才原子提交目标文件；不覆盖源视频和既有目标。
- 取消、正常退出和失败均终止子进程并清理临时文件；非正常中断会保留严格限定命名的恢复记录，下次启动可定位或清理。
- 60 秒没有新进度时显示“可能卡住”，但不擅自终止长时间编码。

## 媒体实现

- 输入：MP4 容器，H.264/AVC 或 H.265/HEVC，支持有声和无声源。
- 预览：Rust 本机回环 HTTP 服务完整实现单/多/开放/suffix Range；只发布当前源的随机令牌 URL，以固定 64 KiB 缓冲区流式读取，不再依赖 Tauri 默认资源协议。
- 精确策略：FFmpeg 顺序解码整个源视频，按原时间轴筛选保留帧，再统一重新编码；不依赖关键帧截断。
- 视频边界吸附到源帧，误差目标不超过一帧；音频按 32 个采样的小块筛选。
- H.264 输入保持 H.264 编码族，H.265 输入保持 H.265 编码族。
- Mac 优先 VideoToolbox；Windows 依次试探 NVENC、QSV、AMF、Media Foundation，每个候选编码器先执行真实一帧编码。
- 自动验收检查：只有一条视频流、音频流数一致、起始时间近零、输出大小合理、编码族一致、总时长、音画差不超一帧，并解码开头、结尾及每个连接点两侧。

## 自动与本地验收结果

| 验证项 | 结果 |
|---|---|
| Svelte/TypeScript | 0 errors，0 warnings |
| Vitest | 9 个文件，36 passed |
| Rust 默认测试 | 47 passed，4 ignored，0 failed |
| FFmpeg 媒体重型测试 | 3 passed，0 failed（单独显式执行） |
| Rust Clippy / rustfmt | `-D warnings` 与格式检查均通过 |
| Windows x64 Clippy | `cargo xwin clippy --all-targets -- -D warnings` 通过 |
| 30 fps H.264/H.265 + AAC | 精确导出、时长、编码族、音画差、连接点解码通过 |
| 29.97 fps H.264 无音频 | 通过 |
| 60 fps H.265 + AAC | 通过，音画差不超一个源帧 |
| H.265 Main10 + AAC | 通过，输出验证为 8-bit |
| 逐帧内容映射 | 91 个输出帧逐一对应预期源帧，删除内容未混入，顺序一致 |
| 取消与既有目标保护 | 通过，partial/filter 已清理，既有目标不变 |
| 保存失败故障注入 | 通过，项目状态与撤销历史一并回滚 |
| 6 小时项目 / 101 个区间 | 保存、加载、5 小时播放位置恢复通过 |
| 合成 3 小时 4K H.265 + AAC，>11 GiB | 真实大文件 probe + 首尾指纹验收通过，测试耗时约 0.10 s，源文件长度不变 |
| 11 GiB 稀疏 4K H.265 + AAC 文件 | 最终 `.app` 选择、探测和项目建立通过，无整文件复制 |
| 本地私有多 GB、数小时、5K H.265 MP4 | 未转码/重封装直接预览成功；文件尾 `moov` 可读取，跨小时 seek 为亚秒级，RSS 约百 MB |
| QuickTime Player 导入 | 实际打开验证过的 H.264 和 H.265 输出，两者时长均为 3.734 s |
| 最终 macOS `.app` | 启动成功，正常退出，无 FFmpeg/FFprobe 进程遗留 |
| DMG / ZIP | DMG CRC 、镜像内 `.app` 深度签名、sidecar/许可文件和 ZIP 完整性均通过 |
| Windows x64 主程序 | 交叉链接成功，PE32+ GUI x86-64 |
| Windows x64 NSIS | 交叉构建包真实启动失败并撤回；新的 Windows 原生包尚未生成 |

大文件探测和项目恢复仍由合成样片覆盖；本次另用用户原始课程录屏完成了真实大文件预览与随机跳转回归。没有对课程内容或三小时完整导出做验收。完整操作和环境记录见 `docs/test-reports/` 下的平台报告。

2026-08-12 的 0.1.0 内部包加入单一专业时间轴、连续 Scrub、锚点缩放、细导航条、可见起止按钮、预览 Seek 合并和窗口缩放时的内部组件自适应。Windows 交叉链接仍可用于源码检查，但交叉 NSIS 打包不再属于发布验收；以后 Windows 安装包必须在 Windows 原生构建并完成静默安装、文件核对和卸载冒烟。

## 产物

- `src-tauri/target/release/bundle/dmg/SpyCut_0.1.0_aarch64.dmg`
- `src-tauri/target/release/bundle/macos/SpyCut_0.1.0_aarch64.zip`
- `docs/release/SpyCut_0.1.0_checksums.txt`

## 仍需外部环境验收

以下事项无法在当前 Apple Silicon Mac 上代替完成：

- Windows 10/11 x64 上安装、WebView2、H.264/H.265 预览、GPU 编码和完整导出；
- 用户真实 3 小时课程录屏的内容检查、完整导出耗时、温度和峰值磁盘验收；
- 导入用户实际使用的专业剪辑软件（本机仅有 QuickTime Player 可用）；
- Intel Mac（如果决定支持）；
- Apple Developer ID / Windows 正式签名、macOS 公证和公开发行的法务/专利审查。

因此，当前 Mac 成品可用于这台 Apple Silicon Mac 的内部试剪；已撤回的 Windows 交叉构建安装包不得继续使用。新的 Windows 原生包必须先通过自动安装/卸载冒烟，再交给真机做媒体功能验收。

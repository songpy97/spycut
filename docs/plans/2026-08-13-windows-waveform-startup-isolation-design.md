# Windows 波形启动隔离设计

## 问题与证据

0.1.2 的 Windows 日志显示，H.265 源视频的探测、指纹、项目读取、保存和预览发布均已完成，但进程在 `source_open_completed` 之后异常终止。日志没有 `waveform_started`，说明 FFmpeg 波形解码尚未进入；同时波形功能引入前后的播放器、视频适配器和预览服务没有变化。因此，本次修复把回归边界定位在导入后同时挂载编辑界面、启动波形 IPC 和加载预览的前端时序，而不是把问题归因于用户未安装 FFmpeg。

## 修复方案

Windows 导入项目后不再自动调用 `get_audio_waveform`。前端先应用会话、设置预览地址并等待播放器触发 `loadedmetadata`；时间轴波形区显示“视频加载完成后可生成音频波形”。预览加载成功后显示可操作的“生成波形”按钮，用户点击时才调用现有的受监督 FFmpeg 波形命令。macOS 保持导入后自动生成波形，避免无证据地改变已稳定的平台行为。

前端通过受控的 `waveform_lifecycle` 诊断事件同步记录 `session_applied`、`preview_loaded`、`request_prepared` 和 `request_dispatched`。日志不包含源文件名、路径、预览 URL 或项目 ID。波形失败继续保持非阻断，视频编辑和导出不受影响。

## 验证

- App 测试覆盖 Windows 导入不自动请求、预览加载后可手动请求，以及非 Windows 仍自动请求。
- TimelineEditor 测试覆盖手动生成按钮的显示、禁用和事件分发。
- Rust 测试覆盖新增诊断事件种类仍受白名单限制。
- 运行 `pnpm check`、真实 FFmpeg 波形测试、Windows 目标 Clippy、生产构建和 `git diff --check`。
- 发布 0.1.3 后要求 Windows 原生安装/卸载冒烟通过；真实 Windows 上以“导入不闪退、点击后开始波形生成”为最终媒体验收标准。

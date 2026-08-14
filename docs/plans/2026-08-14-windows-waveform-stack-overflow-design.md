# Windows 波形命令栈溢出修复设计

## 背景与证据

Windows 0.1.4 真机日志显示，两次手动波形请求都在前端记录 `request_dispatched` 后异常终止，后端没有写出 `waveform_started`、`waveform_failed`、`waveform_completed` 或 `rust_panic`。两次输入分别为约三小时的 H.265 和约 114 秒的 H.264，因此故障与视频时长、视频编码族及返回波形大小无稳定相关性，且发生在 FFmpeg 启动之前。

编译器类型大小输出显示，`extract_audio_waveform` future 为 66,032 字节，`get_audio_waveform` future 为 66,192 字节，Tauri IPC 包装后的 future 为 132,392 字节。主要来源是跨 `await` 保存的 64 KiB 固定数组。Tauri release 路径直接按值包装异步任务，而 Windows x64 默认线程栈较小，这与“进入命令前异常退出且无 Rust panic”的栈溢出表现一致。

## 方案比较

1. 只增大 Windows 主线程栈：可以缓解当前崩溃，但保留超大 future，其他调度线程仍可能受影响，也掩盖了不必要的栈占用。
2. 只在 Tauri 命令边界 `Box::pin`：可以缩小外层 command future，但波形提取 future 本身仍携带 64 KiB 固定数组，后续调用方式变化可能重新暴露问题。
3. 将读取缓冲改为堆分配，并在命令边界显式装箱：从根源缩小提取 future，同时使 Tauri IPC 只持有指针大小的内部 future。该方案改动局部，不改变 PCM 分块、峰值算法、超时或 FFmpeg 参数。

采用方案 3。

## 设计

- `audio_waveform.rs` 将 `[u8; 64 * 1024]` 改为等长的 `Vec<u8>`，保持每次读取上限和流式内存边界不变。
- 增加稳定的 future 大小回归测试；测试只构造 future 而不轮询，不启动 FFmpeg。阈值设为 4 KiB，用于阻止大型固定缓冲再次跨 `await` 进入状态机。
- `get_audio_waveform` 在函数入口首先同步记录 `waveform_command_entered stage=session_validation`，用于区分 IPC 调度失败和会话校验失败。
- 命令调用提取器时使用 `Box::pin(...).await`，避免 Tauri command future 内联携带完整提取状态机。
- 保持现有 `waveform_started`、`waveform_failed` 和 `waveform_completed` 语义；不改变前端、IPC 参数/返回契约、项目持久化或 Windows 手动触发策略。

## 验证

- 先运行新增 future 大小测试并确认旧实现失败。
- 修改后运行音频波形 Rust 单元测试和真实 FFmpeg 忽略测试。
- 运行 `pnpm check`、`git diff --check`。
- 使用编译器类型大小输出复核 `extract_audio_waveform` 和 Tauri IPC future 已显著缩小。
- Windows 真机仍需确认：点击后出现 `waveform_command_entered` 和 `waveform_started`，波形成功完成或以可恢复错误返回，且不再异常退出。

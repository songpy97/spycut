# 本地诊断与崩溃日志设计

## 目标

为 Windows 闪退调查提供持久、可取回且不泄露课程信息的本地证据。日志要能回答：应用何时启动、上次是否异常结束、导入和波形分析执行到哪一步、FFmpeg 是否启动/退出/超时、Rust 或前端是否出现未捕获错误。日志功能不能成为新的启动或编辑故障点。

## 方案

Rust 基础设施层新增 `DiagnosticLog`。日志保存在 Tauri 应用数据目录的 `diagnostics/spycut.log`，启动时同步追加并在每条记录后刷新；超过 5 MiB 时轮换为 `spycut.previous.log`，只保留两份。运行标记文件在启动时创建，只有收到正常窗口关闭流程并完成子进程回收后才删除；若下次启动仍存在，则记录 `previous_session_unclean`。panic hook 同步记录 panic 位置、脱敏后的消息与 backtrace。所有事件使用固定事件名和受控字段，不记录视频路径、文件名、预览 URL、令牌或画面内容；来自前端的自由文本限制长度并过滤 URL 与绝对路径。

Tauri 增加 `get_diagnostic_status` 和 `record_frontend_diagnostic`。前端监听 `window.error` 与 `unhandledrejection`，尽力把未捕获错误写入 Rust 日志；应用标题栏增加“诊断日志”入口，通过现有 opener 在系统文件管理器中定位日志。若日志系统本身不可用，应用仍继续运行并显示普通错误提示。

波形命令记录开始、成功、失败、耗时和峰值数量。与此同时，把 Windows 媒体子进程创建集中到一个 helper，并设置 `CREATE_NO_WINDOW`，修复已经确认的 FFmpeg/FFprobe 黑色控制台窗口回归。该设置覆盖导入、波形、诊断、编码器探测、导出和验收的生产子进程。

## 能力边界

Rust panic、前端未捕获异常和最后一条业务 breadcrumb 可由日志记录。进程被强制终止、断电、WebView2/本机 DLL 访问冲突等无法保证写出完整堆栈；这类情况会留下未清理运行标记与最后一条已刷新记录，仍需结合 Windows 事件查看器中的异常模块和异常码。

## 验证

- Rust 测试覆盖写入与刷新、大小轮换、异常运行标记、正常退出清理、路径/URL 脱敏。
- 前端测试覆盖全局错误上报和“诊断日志”入口。
- 波形测试核对开始/成功/失败 breadcrumb；Windows 目标 Clippy/链接检查验证 `CREATE_NO_WINDOW` 条件编译。
- 运行 `pnpm check`、真实 FFmpeg 波形测试和 `git diff --check`，并检查工作树无意外日志文件。

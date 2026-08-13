# Changelog

## 0.1.4 Windows 波形按钮修复 - 2026-08-14

- 修复“生成音频波形”按钮的鼠标事件穿透到时间轴、导致按钮无法点击的问题。
- 保留波形轨道空白区域的拖动定位行为，仅让按钮本身独立接收指针事件。

## 0.1.3 Windows 波形隔离修复 - 2026-08-13

- Windows 导入视频时不再自动启动音频波形分析，先让播放器和剪辑界面完成初始化。
- 视频预览加载完成或明确失败后，可从波形轨道手动点击“生成音频波形”。
- 新增会话应用、预览就绪、波形请求准备与发出阶段的隐私安全诊断记录。

## 0.1.1 开源预览版 - 2026-08-13

- 修复干净 GitHub macOS runner 未预先创建 DMG 目录时的打包失败。
- Windows 安装包改为原生构建，并通过 NSIS 自检、静默安装、内容核对和静默卸载。

## Open-source release

- Added the MIT license, contribution and security policies, and Dependabot configuration.
- Added tag-triggered native macOS/Windows packaging with an automatic GitHub Release after both targets pass.
- Sanitized local paths and private media details from the public source snapshot.

## 0.1.0 内部测试包更新 - 2026-08-12

- 将重复的局部时间轴与全局概览合并为一条专业编辑时间轴和全片细导航条。
- 新增播放头连续 Scrub、Pointer Capture/取消、边缘自动平移和合并预览 Seek。
- 新增鼠标锚点连续缩放、水平平移、缩放滑杆与“适配全片”。
- 将删除起点、终点、取消、撤销与重做集中到可见工具栏；保存失败保留待完成起点。
- 强化区间边界逐帧吸附、单次提交以及导出期间的完整编辑锁定。
- 重新生成并校验 Apple Silicon macOS DMG/ZIP；同期的 Windows x64 交叉构建 NSIS 后续因真实 Windows 完整性检查失败而撤回。

## 0.1.0 - 2026-08-10

- 交付原时间轴不可重排的删除区间标记、精确调整和 100 步撤销/重做。
- 支持 MP4 H.264/H.265、有声/无声、29.97/30/60 fps 与 Main10 明示回退。
- 使用 FFmpeg 顺序解码、原时间轴筛选和统一重编码，避免关键帧偏移。
- 新增连接点逐处复核、未复核二次确认和 VFR/多流/音频/10-bit 警告。
- 新增磁盘预检、同盘 partial、自动媒体验收、取消、卡住提示和中断恢复记录。
- 新增重启后最近项目恢复、源文件首尾指纹和并发/迟到请求保护。
- 用仅监听本机的 Rust HTTP Range 流替换 Tauri 默认资源协议，支持数 GB、尾部 `moov` 的 MP4 预览与跨小时随机跳转。
- 交付 Apple Silicon macOS DMG/ZIP；Windows 安装器改由 Windows 原生构建和安装/卸载冒烟后交付。

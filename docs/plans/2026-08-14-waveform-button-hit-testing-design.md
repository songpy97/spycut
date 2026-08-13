# 波形生成按钮命中修复设计

## 问题

Windows 0.1.3 已能稳定导入原问题视频，但“生成音频波形”按钮无法点击，点击位置只会移动时间轴。原因是按钮位于 `.timeline-waveform` 内，而该容器为了让波形空白区域复用时间轴 Scrub，设置了 `pointer-events: none`；按钮没有显式恢复指针命中，事件因此穿透到下层时间轴。

## 方案

保持波形容器的 `pointer-events: none`，避免破坏波形空白区域点击和拖动定位。仅对 `.timeline-waveform-status button` 设置 `pointer-events: auto`、相对定位和更高层级；组件已有的 `pointerdown` 与 `click` 停止冒泡继续保留，因此按钮命中后不会启动时间轴手势。相比让整个状态层接收事件，这个方案只改变按钮的命中区域，范围最小。

## 验证

新增 CSS 回归测试，同时断言波形容器继续穿透、按钮恢复命中。运行前端组件测试、完整 `pnpm check`、Windows x64 目标 Clippy和生产构建。版本提升为 0.1.4，通过原生 Windows 安装/卸载冒烟后发布；最终以真实 Windows 点击按钮能开始波形分析为验收标准。

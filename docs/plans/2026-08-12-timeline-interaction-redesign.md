# SpyCut 单时间轴交互重构 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将当前“精确时间轴 + 全局概览”两条相似轨道重构为一条专业、可连续拖动和缩放的编辑时间轴，并为删除区间起点、终点提供始终可发现的按钮操作。

**Architecture:** 保持 Rust 项目模型、删除区间命令和导出链路不变，只重构 Svelte 前端时间轴、播放器预览 Seek 和页面编排。时间轴视口、播放头和指针手势相互独立；连续拖动只更新临时 UI 和预览位置，松手后才精确 Seek、保存播放位置或提交一次区间调整。

**Tech Stack:** Svelte 5、TypeScript、Vite、Vitest、Testing Library、Tauri 2、HTML Video、Pointer Events、`requestAnimationFrame`、现有 Rust/Tauri commands。

---

## 0. 文档状态

- 日期：2026-08-12
- 状态：实现与自动化验证已完成；macOS Tauri 触控板和 Windows 真机验收待执行
- 适用范围：SpyCut 主编辑界面的播放、定位、删除区间标记和边界调整
- 不包含：复核页重设计、导出流程重设计、媒体格式扩展、波形或缩略图缓存
- 目标平台：macOS 与 Windows 桌面端
- 实现原则：先写失败测试，再实现最小行为；每个阶段均保持项目可运行
- 当前工作树：编写本文档前为干净状态

实现记录（2026-08-12）：

- 已用 `TimelineEditor` 和内置细导航条替换原 `DetailTimeline` / `OverviewTimeline`。
- 已接入连续 Scrub、Pointer Capture、丢失捕获与 Esc 取消、鼠标锚点缩放、水平平移、边缘自动平移和帧吸附边界调整。
- 已把起点、终点、取消、撤销、重做和缩放控制集中到时间轴工具栏。
- 已在 `PlayerPane` 与 `HtmlVideoAdapter` 实现合并预览 Seek 和带超时的最终精确 Seek。
- 已增加时间轴数学、工具栏、手势、播放器适配及 App 保存失败回归测试。
- 已通过 960×680 与 1440×900 本地浏览器验收；这不等同于 macOS Tauri 触控板或 Windows WebView2 真机验收。

本文档是开发实施依据，不代表其中列出的手工验收已经完成。任何平台实机结果只能在实际执行后写入验收报告。

## 1. 背景与问题定义

### 1.1 用户反馈

当前界面达到“可以使用”的最低标准，但时间轴操作不符合桌面专业剪辑软件的直觉：

1. 底部同时出现“精确时间轴”和“全局概览”，两者都有轨道、刻度、播放头和红色删除区间，看起来像重复控件。
2. 鼠标进入时间轴显示十字加号，无法表达“定位、拖动播放头”的真实动作。
3. 时间轴只能单击跳转，不能按住鼠标连续拖动当前时间点。
4. 缩放只有按钮和少量固定档位，长录屏中无法顺畅地从全局定位过渡到逐帧调整。
5. 删除起点、终点虽然已经存在于播放器控制条，但位置弱、状态弱，用户仍然感觉只能靠快捷键完成标记。
6. 时间轴缺少悬停时间、拖动反馈、自动平移、适配全片和清晰的工具状态。

### 1.2 代码根因

当前实现中的关键限制：

- `src/App.svelte` 同时挂载 `DetailTimeline` 和 `OverviewTimeline`。
- `src/app.css` 用固定的 `152px + 110px` 两行布局展示两条时间轴。
- `DetailTimeline.svelte` 的轨道只处理 `pointerdown`，没有播放头拖动生命周期。
- 两条轨道都使用 `cursor: crosshair`。
- 播放头设置了 `pointer-events: none`，无法直接抓取。
- `windowStart` 每次都由 `playheadUs - effectiveWindow / 2` 计算，播放头改变会反向推动整个视口改变。
- 缩放只在 6 个离散时间窗口之间跳转。
- 区间边界拖动使用全局 `window.pointermove`，缺少 Pointer Capture、`pointercancel` 和显式手势状态。
- `HtmlVideoAdapter.seekTo` 每次都等待一个新的 `seeked` 事件；连续拖动若直接复用会堆积等待和过期位置。
- 当前测试没有覆盖时间轴坐标、缩放、Scrub、Pointer 生命周期和标记按钮状态机。

### 1.3 根本判断

本次不是简单的 CSS 调整。必须同时改变：

- 时间轴信息架构；
- 视口状态模型；
- Pointer 手势模型；
- 播放器预览 Seek 策略；
- 删除区间标记状态反馈；
- 测试覆盖方式。

只给现有轨道补 `pointermove` 会继续受到“视口追随播放头”的影响，无法形成稳定、专业的操作体验。

## 2. 目标与非目标

### 2.1 必须达到的目标

1. 主界面只出现一条有完整刻度、播放头和编辑能力的时间轴。
2. 保留跨数小时快速导航能力，但表现为细导航条，而不是第二条时间轴。
3. 用户可在标尺、轨道空白处和删除区间主体上按住左键连续拖动播放头。
4. 播放头顶部手柄可直接抓取。
5. 缩放连续、以鼠标位置为锚，并同时提供可发现的滑杆、`− / +` 和“适配全片”按钮。
6. 删除起点、终点和取消按钮位于时间轴工具栏，具有完整状态反馈。
7. 删除区间主体仍不可移动，只能选择、删除或调整左右边界。
8. 边界调整继续吸附到源视频帧，松手只提交一次后端命令。
9. 拖动中视频预览跟手，不因过期媒体事件跳回。
10. 导出锁定期间禁止所有区间编辑和标记操作。
11. 保持现有撤销、重做、快捷键、自动保存和复核失效规则。
12. 不增加长视频打开时的全片扫描、缩略图或波形缓存。

### 2.2 明确不做

- 不实现多轨道。
- 不把源视频切成可移动 Clip。
- 不实现 ripple editing。
- 不允许拖动、复制、交换或重新排列删除区间。
- 不增加剃刀、分割、转场、素材、字幕或关键帧工具。
- 不生成完整音频波形。
- 不生成全片缩略图胶片。
- 不修改 Rust 项目 schema。
- 不新增 Tauri command 或 event。
- 不修改 FFmpeg 导出策略。
- 不改变支持的输入格式。
- 不在本次加入快捷键自定义设置页。
- 不做与时间轴无关的视觉重构。

### 2.3 不可破坏的产品约束

- 原始时间轴不可重排。
- 项目仍只保存规范化删除区间。
- 保留区间仍由删除区间补集确定。
- 源文件仍只读。
- 重叠和相邻删除区间仍由 Rust 合并。
- 修改区间后复核状态仍失效。
- 导出期间仍锁定项目快照。
- 前端预览交互不能改变精确导出的帧映射规则。

## 3. 目标界面

### 3.1 页面布局

```text
┌─────────────────────────────────────────────────────────────────────┐
│ ↶ ↷ │ [设删除起点 I] [设删除终点 O] [取消] │ 当前时间 │ 适配全片 −━━＋ │
├─────────────────────────────────────────────────────────────────────┤
│ 01:04:50        01:05:00        01:05:10        01:05:20   时间标尺 │
├─────────────────────────────────────────────────────────────────────┤
│ 锁定的原始视频  ███████▒▒DELETE▒▒██████████████▒DELETE▒██████████ │
│                                ▼ 当前播放头                         │
├─────────────────────────────────────────────────────────────────────┤
│ ────────────────[──── 当前可视窗口，可拖动 ────]────────────────── │
└─────────────────────────────────────────────────────────────────────┘
```

布局规则：

- 时间轴总高度目标为 188–200px。
- 工具栏高度 38–42px。
- 标尺高度 28–32px。
- 主轨高度 70–80px。
- 导航条视觉高度 10–14px，实际命中区域不小于 24px。
- 删除原有“精确时间轴”和“全局概览”两个标题。
- 删除第二套时间刻度和第二个播放头。
- 页面腾出的高度归还给视频预览区域。

### 3.2 工具栏顺序

从左到右：

1. 撤销、重做。
2. 分隔线。
3. `设删除起点 I`。
4. `设删除终点 O`。
5. 待完成状态下出现 `取消 Esc`。
6. 当前播放时间码与总时长。
7. 弹性空白。
8. `帧吸附` 状态提示；V1 固定开启，不做开关。
9. `适配全片`。
10. 缩小按钮。
11. 对数缩放滑杆。
12. 放大按钮。

播放器下方 `TransportControls` 只保留播放、前后 5 秒、时间码和倍速，不再重复显示标记与撤销按钮。

### 3.3 视觉层级

- 原始源轨：中性深灰，显示“源时间轴锁定”。
- 已完成删除区间：实心或轻斜纹红色。
- 当前选中区间：更亮边框与两端手柄。
- 待完成删除区间：琥珀色或红色透明斜纹，不与已保存区间混淆。
- 当前播放头：高对比细线、顶部三角形和时间标签。
- 悬停预览头：低透明度细线和时间浮层。
- 导航条：无刻度、无独立播放头；只显示全局轨道、删除点和当前视口滑块。
- 禁用状态：降低对比度并阻止 Pointer 操作，但保留状态文案。

## 4. 交互规格

### 4.1 鼠标与触控板操作矩阵

| 输入 | 目标 | 行为 |
|---|---|---|
| 鼠标移动 | 标尺或主轨 | 显示悬停时间和预览播放头 |
| 左键单击 | 标尺或主轨空白 | 播放头精确跳转 |
| 左键按住并左右拖动 | 标尺、主轨或区间主体 | 连续 Scrub |
| 左键拖动 | 播放头顶部手柄 | 连续 Scrub |
| 左键单击 | 删除区间主体 | 选中区间并定位播放头 |
| 左键拖动 | 删除区间主体 | Scrub，不移动区间 |
| 左键拖动 | 已选区间左/右手柄 | 预览边界调整 |
| 左键拖动 | 导航条视口滑块 | 平移可视时间窗口 |
| 单击 | 导航条滑块外 | 将视口中心移动到目标位置 |
| 触控板横向滑动 | 时间轴 | 平移时间轴 |
| `Shift + 鼠标滚轮` | 时间轴 | 水平平移 |
| `Alt/Option + 滚轮或横向滑动` | 时间轴 | 以鼠标时间为锚连续缩放 |
| 触控板 pinch | 时间轴 | 在 WebView 事件支持可靠时映射为同样的锚点缩放 |
| 双击 | 删除区间主体 | V1 不定义额外行为 |
| 右键 | 时间轴 | V1 不新增上下文菜单 |

只有指针位于时间轴内部时才拦截缩放或横移手势，避免影响页面其他区域。

### 4.2 Cursor 规范

| 区域/状态 | Cursor |
|---|---|
| 普通轨道 | `default` |
| 可拖播放头手柄 | `grab` |
| 正在 Scrub | `grabbing` |
| 区间边界 | `ew-resize` |
| 导航条滑块 | `grab` / `grabbing` |
| 明确按钮 | `pointer` |
| 导出锁定 | `not-allowed`，仅用于真正不可操作的整体锁定 |

禁止在主时间轴继续使用 `crosshair`。区间主体也不再使用 `not-allowed`，因为它仍然可以被选择和用于 Scrub。

### 4.3 Scrub 生命周期

```text
idle
  └─ pointerdown
       ├─ 记录 pointerId
       ├─ setPointerCapture
       ├─ 记录拖动前是否正在播放
       ├─ 暂停播放器
       └─ scrubbing
            ├─ pointermove → 更新乐观播放头 → 合并预览 Seek
            ├─ pointercancel / Escape → 回到拖动前位置
            └─ pointerup
                 ├─ 精确 Seek 到最终位置
                 ├─ 保存一次 lastPlayheadUs
                 ├─ 如拖动前正在播放则恢复播放
                 └─ idle
```

规则：

- `pointermove` 必须通过 Pointer Capture 继续接收，即使鼠标短暂离开轨道。
- 拖动中 `playheadUs` 先在 UI 中立即变化，不等待视频 `seeked`。
- 拖动中每个动画帧最多向播放器提交一次最新目标。
- 新目标覆盖旧目标，不排队执行所有中间位置。
- `handleTime` 在 Scrub 期间忽略播放器发回的旧时间。
- 松手后执行一次精确 Seek；只有这次成功后才恢复播放。
- 取消 Scrub 不保存播放位置。
- 播放位置持久化继续使用现有节流，但一次拖动只能产生一次最终候选值。

### 4.4 播放跟随

当前实现始终以播放头为中心重算视口，本次必须取消。

新规则：

- 用户主动平移或缩放后，视口保持稳定。
- 播放头在视口内移动时不改变视口。
- 播放头达到视口右侧约 85% 时，视口向前移动约 70% 的一个页面。
- 播放头通过外部操作跳到视口之外时，把目标放到视口 50% 位置。
- 用户点击“适配全片”后显示完整源时长。
- 用户开始边界调整时不自动居中；只有指针进入边缘自动平移区域才移动视口。

### 4.5 删除起止状态机

```text
Idle
  ├─ 设删除起点 / I
  │    └─ Pending(startUs)
  │         ├─ 重设起点 / I → Pending(newStartUs)
  │         ├─ 取消 / Esc → Idle
  │         ├─ playhead <= startUs → 终点禁用
  │         └─ 设删除终点 / O
  │              ├─ 保存成功 → Idle + 选中规范化结果
  │              └─ 保存失败 → 保持 Pending
  └─ 其他编辑行为
```

按钮文案与状态：

| 状态 | 起点按钮 | 终点按钮 | 取消按钮 | 轨道反馈 |
|---|---|---|---|---|
| Idle | `设删除起点 I` | 禁用 | 隐藏 | 无 |
| Pending 且播放头晚于起点 | `重设起点 I` | 高亮 `完成删除区间 O` | 显示 | 起点到播放头斜纹 |
| Pending 且播放头不晚于起点 | `重设起点 I` | 禁用并解释原因 | 显示 | 只显示起点标记 |
| Saving | 禁用 | 显示保存中 | 禁用 | 保留斜纹 |
| Export locked | 全部禁用 | 全部禁用 | 全部禁用 | 显示锁定提示 |

保存失败不能提前清除 `pendingStartUs`。只有收到成功的 `SessionProjection` 后才进入 Idle。

### 4.6 区间选择和边界调整

- 点击区间主体时先选中，再把播放头定位到点击时间。
- 区间主体不提供平移手势。
- 选中后显示左右独立手柄。
- 手柄不得嵌套在区间主体 `button` 内，避免嵌套交互元素和错误辅助技术语义。
- 手柄视觉宽度可为 6–8px，实际命中宽度至少 16px。
- 拖动预览仅保存在组件临时状态。
- 松手只派发一次 `resize`。
- `pointercancel`、`Escape` 和组件销毁均恢复原始区间。
- 左边界最大值为 `endUs - frameDurationUs`。
- 右边界最小值为 `startUs + frameDurationUs`。
- 所有时间继续限制在 `[0, durationUs]`。
- 边界调整时吸附到视频帧。
- 松手后 Rust 仍负责最终规范化、相邻合并和复核失效。
- 如果规范化导致区间 ID 改变，前端选中覆盖本次目标范围的规范化区间。

### 4.7 边缘自动平移

Scrub 和边界调整都支持边缘自动平移：

- 左右各 24px 为触发区。
- 速度按进入触发区的深度连续变化。
- 最低速度约为可视跨度的 2%/秒。
- 最高速度约为可视跨度的 40%/秒。
- 使用 `requestAnimationFrame` 驱动。
- 视口到达 0 或源时长末尾后停止。
- 指针离开触发区、松手、取消或组件销毁时必须停止动画帧。

## 5. 时间轴数学模型

### 5.1 核心类型

在 `src/lib/timeline/viewport.ts` 定义纯函数类型：

```ts
export interface TimelineViewport {
  startUs: number;
  spanUs: number;
}

export interface TimelineBounds {
  durationUs: number;
  frameDurationUs: number;
}

export interface TimelineTick {
  timeUs: number;
  major: boolean;
  label: string | null;
}
```

约束：

- `startUs >= 0`。
- `spanUs >= frameDurationUs * 20`。
- `spanUs <= durationUs`。
- `startUs + spanUs <= durationUs`。
- duration 小于最小跨度时直接显示完整 duration。

### 5.2 坐标换算

```ts
timeUs = viewport.startUs
  + clamp((clientX - rect.left) / rect.width, 0, 1) * viewport.spanUs

xPx = ((timeUs - viewport.startUs) / viewport.spanUs) * rect.width
```

用于播放头和边界的目标时间最后执行：

```ts
snappedUs = clamp(
  Math.round(timeUs / frameDurationUs) * frameDurationUs,
  0,
  durationUs
)
```

悬停时间可以先显示未吸附值，但实际播放头和删除边界必须使用吸附值，避免视觉反馈与最终提交不一致。

### 5.3 鼠标锚点缩放

设：

- 旧视口为 `startUs`、`spanUs`。
- 鼠标锚点在轨道内比例为 `anchorRatio`。
- 锚点时间为 `anchorUs = startUs + anchorRatio * spanUs`。
- Wheel 正值表示缩小，负值表示放大。

```ts
scale = Math.exp(normalizedDelta * 0.0015)
nextSpanUs = clamp(spanUs * scale, minSpanUs, durationUs)
nextStartUs = clamp(
  anchorUs - anchorRatio * nextSpanUs,
  0,
  durationUs - nextSpanUs
)
```

验收要求：缩放前后的鼠标锚点时间误差不超过一个源视频帧。

### 5.4 缩放滑杆

滑杆值 `t` 范围为 `[0, 1]`，采用对数映射：

```ts
spanUs = minSpanUs * Math.pow(durationUs / minSpanUs, 1 - t)
```

- `t = 0`：适配全片。
- `t = 1`：最大放大。
- `− / +` 每次按 1.25 倍调整跨度。
- 按钮和滑杆都以当前播放头为锚；播放头不在视口时以视口中心为锚。

### 5.5 动态刻度

候选刻度从以下序列选择：

- 1、2、5、10 帧；
- 100ms、250ms、500ms；
- 1s、2s、5s、10s、15s、30s；
- 1min、2min、5min、10min、15min、30min；
- 1h。

选择规则：

- 主刻度间距目标不小于 90px。
- 次刻度间距目标不小于 18px。
- 显示范围小于 10 秒时显示毫秒或帧相关信息。
- 超过一小时时继续使用现有多小时时间格式。
- 标尺只渲染当前视口内刻度，不遍历完整三小时的每一帧。

### 5.6 导航条

- 全局轨道长度始终代表 `[0, durationUs]`。
- 视口滑块左侧比例为 `startUs / durationUs`。
- 视口滑块宽度比例为 `spanUs / durationUs`。
- 极小视口的视觉滑块允许小于 24px，但额外提供 24px 透明命中区，不能通过夸大视觉宽度破坏映射。
- 删除区间显示为最小 1px 的红色标记。
- 导航条不显示第二个播放头和时间标签。

## 6. 前端架构

### 6.1 组件结构

```text
App.svelte
├── PlayerPane.svelte
│   └── HtmlVideoAdapter
├── TransportControls.svelte
├── IntervalList.svelte
└── TimelineEditor.svelte
    ├── TimelineToolbar.svelte
    ├── 时间标尺
    ├── 源轨与删除区间
    ├── 播放头与悬停头
    └── 全局导航条
```

迁移完成后删除：

- `src/lib/components/DetailTimeline.svelte`
- `src/lib/components/OverviewTimeline.svelte`

不引入新的全局 Store。项目领域状态继续由 `App.svelte` 编排；视口和手势等瞬时 UI 状态由 `TimelineEditor.svelte` 管理。

### 6.2 状态所有权

| 状态 | 所有者 | 是否持久化 |
|---|---|---|
| `playheadUs` | `App.svelte` | 节流保存 |
| `pendingStartUs` | `App.svelte` | 否 |
| `selectedId` | `App.svelte` | 否 |
| `deleteIntervals` | Rust session projection | 是 |
| `reviewedIntervalIds` | Rust session projection | 是 |
| `viewport` | `TimelineEditor.svelte` | V1 否 |
| `gesture` | `TimelineEditor.svelte` | 否 |
| `hoverUs` | `TimelineEditor.svelte` | 否 |
| `resizePreview` | `TimelineEditor.svelte` | 否 |
| `scrubbing` | `App.svelte` 与组件事件协作 | 否 |
| `resumeAfterScrub` | `App.svelte` | 否 |

### 6.3 手势联合类型

```ts
type TimelineGesture =
  | { kind: "idle" }
  | {
      kind: "scrub";
      pointerId: number;
      originUs: number;
    }
  | {
      kind: "resize";
      pointerId: number;
      intervalId: number;
      edge: "start" | "end";
      originalStartUs: number;
      originalEndUs: number;
      previewStartUs: number;
      previewEndUs: number;
    }
  | {
      kind: "navigate";
      pointerId: number;
      grabOffsetRatio: number;
    };
```

禁止用多个互不约束的 `isDragging`、`isResizing`、`isPanning` 布尔值表示互斥手势。

### 6.4 组件输入

`TimelineEditor.svelte` 接收：

```ts
export let durationUs: number;
export let frameDurationUs: number;
export let playheadUs: number;
export let intervals: DeleteInterval[];
export let selectedId: number | null;
export let pendingStartUs: number | null;
export let canUndo: boolean;
export let canRedo: boolean;
export let playing: boolean;
export let editLocked: boolean;
```

### 6.5 组件事件

```ts
type TimelineEditorEvents = {
  scrubStart: { playheadUs: number };
  scrubPreview: { playheadUs: number };
  scrubCommit: { playheadUs: number };
  scrubCancel: void;
  select: { id: number; playheadUs: number };
  resize: { id: number; startUs: number; endUs: number };
  markStart: void;
  markEnd: void;
  cancelMark: void;
  undo: void;
  redo: void;
};
```

视口变化不发送到 `App.svelte`，避免把纯 UI 导航混入项目状态。

### 6.6 App 事件流

```text
Timeline scrubStart
→ App 记录 playing
→ App 设置 scrubbing=true
→ PlayerPane.pause()

Timeline scrubPreview(time)
→ App 乐观更新 playheadUs
→ PlayerPane.previewSeekTo(time)

Timeline scrubCommit(time)
→ PlayerPane.seekTo(time)
→ schedulePlayheadSave(time)
→ scrubbing=false
→ 如原来正在播放则 PlayerPane.play()
```

`handleTime` 的第一条规则应是：

```ts
if (scrubbing) return;
```

防止 `requestVideoFrameCallback`、`timeupdate` 或过期 `seeked` 覆盖拖动中的乐观播放头。

## 7. 播放器预览 Seek

### 7.1 接口调整

在 `MediaPlayerAdapter` 增加：

```ts
previewSeekTo(seconds: number): void;
```

保留：

```ts
seekTo(seconds: number): Promise<void>;
```

两者语义不同：

- `previewSeekTo`：允许近似、合并、立即返回，用于拖动预览。
- `seekTo`：等待最终位置稳定，用于单击跳转和拖动提交。

### 7.2 PlayerPane 合并策略

`PlayerPane.svelte` 保存：

```ts
let pendingPreviewUs: number | null = null;
let previewFrameRequest: number | null = null;
```

`previewSeekTo` 每次只覆盖 `pendingPreviewUs`。若尚未安排动画帧，则安排一次；动画帧执行时读取最新值并调用 Adapter。

组件销毁和切换视频源时必须取消未执行的动画帧。

### 7.3 HtmlVideoAdapter 精确 Seek

当前 `seekTo` 仅等待 `seeked`，存在目标已经等于当前时间时不再触发事件的风险。重构要求：

1. Clamp 到非负秒数。
2. 与当前时间差小于半帧或 0.5ms 时直接成功。
3. 同时监听 `seeked` 和 `error`。
4. 设置不超过 5 秒的超时。
5. 成功、失败或超时均清理事件和 timer。
6. 超时返回可处理错误，不允许 Promise 永久等待。

`previewSeekTo` 不等待事件，只设置最新 `currentTime`。若实机验证 `fastSeek` 对拖动预览明显更流畅，可仅在预览方法中使用；最终 `seekTo` 仍必须使用精确目标。

## 8. 删除区间命令整合

### 8.1 runEdit 返回结果

当前 `runEdit` 只应用 Session，不把结果交回调用者。为了在规范化合并后选择正确区间，应调整为返回：

```ts
Promise<SessionProjection | null>
```

- 成功：返回已应用的 projection。
- Demo：完成本地更新后返回新的 projection。
- 失败：设置错误状态并返回 `null`。

现有调用方可以忽略返回值；`markEnd` 和 `resizeInterval` 使用返回值恢复正确选中项。

### 8.2 完成标记

正确顺序：

1. 读取当前 `pendingStartUs` 和 `playheadUs`。
2. 前端验证终点晚于起点至少一帧。
3. 设置 `markSaving=true`，但不清除 `pendingStartUs`。
4. 调用 `addDeleteInterval`。
5. 失败：保留起点、取消 saving、显示错误。
6. 成功：应用 projection。
7. 在规范化区间中查找覆盖 `[startUs, endUs]` 的区间。
8. 设置 `selectedId`。
9. 清除 `pendingStartUs` 和 saving。

### 8.3 调整后选中项

`resize_delete_interval` 可能把目标区间与邻接区间合并，原 ID 不一定继续存在。成功后按以下规则寻找目标：

```ts
item.startUs <= requestedStartUs
  && item.endUs >= requestedEndUs
```

规范化区间不重叠，因此最多命中一个。命中后选中该 ID；未命中则清空选择并报告异常，不凭旧 ID 猜测。

### 8.4 导出锁

`editLocked` 至少在以下条件成立时为 true：

```ts
exportOpen && !exportResult
```

锁定后：

- 起点、终点、取消、撤销、重做禁用。
- 时间轴仍可查看和定位，但不能创建、删除或调整区间。
- 区间边界不显示可拖 Cursor。
- 全局键盘处理继续阻止编辑快捷键。
- 后端 workflow gate 仍是最终安全边界。

## 9. 可访问性与键盘

### 9.1 语义结构

- 时间轴整体使用 `role="group"` 和明确 `aria-label`。
- 播放头使用独立 `role="slider"`。
- 导航条使用 `role="scrollbar"`，`aria-orientation="horizontal"`。
- 区间主体使用可聚焦按钮或 `role="button"`，但边界手柄必须是它的兄弟元素。
- 左右边界分别使用 `role="slider"`。
- 所有 slider 提供 `aria-valuemin`、`aria-valuemax`、`aria-valuenow` 和格式化后的 `aria-valuetext`。
- 工具栏按钮具有可见文本，不能只依赖图标或 title。
- Focus ring 继续使用现有高对比琥珀色。

### 9.2 键盘行为

保留：

- `Space`：播放/暂停。
- `I` / `[`：设起点或重设起点。
- `O` / `]`：完成删除区间。
- `Esc`：取消待完成起点、当前 Pointer 手势或复核页。
- `Left / Right`：全局前后 1 秒。
- `Shift + Left / Right`：全局前后 5 秒。
- `Cmd/Ctrl + Left / Right`：全局前后 30 秒。
- `J / K / L`：速度控制。
- `Cmd/Ctrl + Z`：撤销。
- `Cmd/Ctrl + Shift + Z`：重做。
- `Delete / Backspace`：移除选中删除区间。

当播放头或边界 slider 获得焦点：

- `Left / Right`：逐帧移动。
- `Shift + Left / Right`：1 秒移动。
- 必须阻止事件继续冒泡到全局快捷键。

输入框、按钮、选择框、可编辑区域和任何 slider 获得焦点时，不执行不相关的全局快捷键。

## 10. 响应式规则

### 10.1 最低窗口

继续支持：

- 最小宽度 960px。
- 最小高度 680px。

在 960–1119px：

- 工具栏按钮缩短为“设起点”“设终点”。
- `kbd` 提示仍保留。
- 可以隐藏“帧吸附”文字，但不能隐藏起止按钮。
- 缩放滑杆允许缩短，`− / +` 和适配全片仍显示。
- 倍速条可继续按现有逻辑隐藏。

### 10.2 宽屏

- 时间轴内容使用全宽，不在右侧区间列表列宽处断开。
- 时间标签避免溢出窗口右边界。
- 播放头接近两端时，时间浮层自动翻转方向。
- 长达三小时的时间码使用等宽数字，避免宽度跳动。

## 11. 文件改动清单

### 11.1 新增

| 文件 | 职责 |
|---|---|
| `src/lib/timeline/viewport.ts` | 纯时间轴坐标、缩放、平移、刻度和导航条计算 |
| `src/lib/timeline/viewport.test.ts` | 纯数学边界测试 |
| `src/lib/components/TimelineEditor.svelte` | 单时间轴主组件 |
| `src/lib/components/TimelineEditor.test.ts` | Pointer、选择、缩放、边界和导航条测试 |
| `src/lib/components/TimelineToolbar.svelte` | 起止、撤销重做、适配和缩放控件 |
| `src/lib/components/TimelineToolbar.test.ts` | 按钮状态与事件测试 |
| `src/lib/components/PlayerPane.test.ts` | 预览 Seek 合并和提交测试 |
| `src/App.test.ts` | Scrub 生命周期、标记保存失败和规范化选中回归 |

### 11.2 修改

| 文件 | 改动 |
|---|---|
| `src/App.svelte` | 单时间轴接线、Scrub 状态、runEdit 返回值、标记成功语义、导出锁 |
| `src/app.css` | 新布局、Cursor、工具栏、导航条、播放头和响应式样式 |
| `src/lib/components/TransportControls.svelte` | 移除标记和撤销重做重复控件 |
| `src/lib/components/PlayerPane.svelte` | 导出 `previewSeekTo`，合并拖动预览 |
| `src/lib/player/MediaPlayerAdapter.ts` | 增加预览 Seek 契约 |
| `src/lib/player/HtmlVideoAdapter.ts` | 实现预览 Seek 和有界精确 Seek |
| `README.md` | 更新按钮、拖动、平移和缩放用法 |
| `docs/SpyCut-V1-开发文档.md` | 把两级时间轴替换为单时间轴 + 导航条 |
| `AGENTS.md` | 更新核心时间轴组件导航和相关验证要求 |

### 11.3 删除

| 文件 | 条件 |
|---|---|
| `src/lib/components/DetailTimeline.svelte` | TimelineEditor 完成接线并通过测试后 |
| `src/lib/components/OverviewTimeline.svelte` | TimelineEditor 完成接线并通过测试后 |

不在同一补丁一开始删除旧组件。先让新组件测试通过并接入 App，再删除旧文件，保证每一步可回退和可验证。

## 12. 实施任务

### Task 1：建立时间轴纯数学模块

**Files:**

- Create: `src/lib/timeline/viewport.test.ts`
- Create: `src/lib/timeline/viewport.ts`

**Step 1: 写坐标换算失败测试**

覆盖：

- 视口起点、中心和终点到像素。
- 像素到时间。
- 超出轨道左右边界的 clamp。
- 29.97fps、17.12fps 和 60fps 帧吸附。

**Step 2: 运行测试确认失败**

Run:

```sh
pnpm exec vitest run src/lib/timeline/viewport.test.ts
```

Expected: FAIL，模块尚不存在。

**Step 3: 实现最小坐标函数**

实现 `clampViewport`、`timeAtClientX`、`xAtTime` 和 `snapToFrame`。

**Step 4: 写锚点缩放、平移和滑杆映射失败测试**

覆盖完整时长、最小跨度、视频两端和锚点误差。

**Step 5: 实现缩放、平移和导航条计算**

实现 `zoomAtAnchor`、`panViewport`、`viewportFromSlider`、`sliderFromViewport`。

**Step 6: 写动态刻度测试并实现**

确认三小时全局视图不会生成逐帧刻度，高倍率视图可以生成帧级刻度。

**Step 7: 运行测试**

Expected: PASS。

**Step 8: Commit**

```sh
git add src/lib/timeline/viewport.ts src/lib/timeline/viewport.test.ts
git commit -m "feat: add timeline viewport model"
```

### Task 2：实现 TimelineToolbar 状态机

**Files:**

- Create: `src/lib/components/TimelineToolbar.svelte`
- Create: `src/lib/components/TimelineToolbar.test.ts`

**Step 1: 写 Idle 状态失败测试**

断言起点可用、终点禁用、取消隐藏。

**Step 2: 写 Pending 状态失败测试**

断言重设起点、终点高亮、取消可用、起点时间可见。

**Step 3: 写无效终点和保存中失败测试**

断言终点不晚于起点时禁用；saving 和 editLocked 时编辑按钮禁用。

**Step 4: 实现最小组件和事件**

事件仅表达用户意图，不在 Toolbar 内修改 App 状态。

**Step 5: 运行测试**

Run:

```sh
pnpm exec vitest run src/lib/components/TimelineToolbar.test.ts
```

Expected: PASS。

**Step 6: Commit**

```sh
git add src/lib/components/TimelineToolbar.svelte src/lib/components/TimelineToolbar.test.ts
git commit -m "feat: add timeline marking toolbar"
```

### Task 3：实现 TimelineEditor 静态结构

**Files:**

- Create: `src/lib/components/TimelineEditor.svelte`
- Create: `src/lib/components/TimelineEditor.test.ts`
- Modify: `src/app.css`

**Step 1: 写单时间轴结构失败测试**

断言：

- 只有一个时间标尺。
- 只有一个播放头。
- 导航条没有独立播放头。
- 删除区间和待完成区间正确渲染。
- editLocked 语义存在。

**Step 2: 实现组件结构**

先只渲染，不接 Pointer 手势。

**Step 3: 实现可访问性结构**

播放头、导航条、区间主体和手柄分别具有合法语义，不嵌套交互按钮。

**Step 4: 添加基础样式**

实现高度、颜色、播放头、区间和导航条，不进行页面接线。

**Step 5: 运行组件测试和 typecheck**

```sh
pnpm exec vitest run src/lib/components/TimelineEditor.test.ts
pnpm typecheck
```

Expected: PASS。

**Step 6: Commit**

```sh
git add src/lib/components/TimelineEditor.svelte src/lib/components/TimelineEditor.test.ts src/app.css
git commit -m "feat: add unified timeline editor"
```

### Task 4：实现连续 Scrub

**Files:**

- Modify: `src/lib/components/TimelineEditor.svelte`
- Modify: `src/lib/components/TimelineEditor.test.ts`

**Step 1: 写 Pointer 生命周期失败测试**

模拟 `pointerdown → pointermove → pointerup`，断言：

- 使用 Pointer Capture。
- 发出一次 start、多次 preview、一次 commit。
- 区间主体拖动是 Scrub，不是 resize。

**Step 2: 写取消失败测试**

覆盖 `pointercancel`、`Escape` 和组件销毁。

**Step 3: 实现联合手势状态**

禁止使用互斥不安全的多个布尔值。

**Step 4: 实现悬停播放头和 Cursor**

确保不存在 `crosshair`。

**Step 5: 运行测试**

Expected: PASS。

**Step 6: Commit**

```sh
git add src/lib/components/TimelineEditor.svelte src/lib/components/TimelineEditor.test.ts src/app.css
git commit -m "feat: support continuous timeline scrubbing"
```

### Task 5：实现连续缩放、平移和导航条

**Files:**

- Modify: `src/lib/components/TimelineEditor.svelte`
- Modify: `src/lib/components/TimelineEditor.test.ts`
- Modify: `src/lib/components/TimelineToolbar.svelte`
- Modify: `src/lib/components/TimelineToolbar.test.ts`

**Step 1: 写 Wheel 缩放失败测试**

断言只有时间轴内部的 `Alt/Option + wheel` 会 preventDefault，并保持鼠标锚点时间。

**Step 2: 写横移失败测试**

覆盖触控板 `deltaX`、`Shift + deltaY`、两端 clamp。

**Step 3: 写适配全片和滑杆失败测试**

断言适配后 `startUs=0` 且 `spanUs=durationUs`。

**Step 4: 写导航条拖动失败测试**

断言只移动视口，不改变播放头和区间。

**Step 5: 实现交互**

Wheel listener 必须为非 passive，以便仅在目标手势时阻止默认行为。

**Step 6: 运行测试**

Expected: PASS。

**Step 7: Commit**

```sh
git add src/lib/components/TimelineEditor.svelte src/lib/components/TimelineEditor.test.ts src/lib/components/TimelineToolbar.svelte src/lib/components/TimelineToolbar.test.ts
git commit -m "feat: add timeline zoom and navigation"
```

### Task 6：实现边界调整和自动平移

**Files:**

- Modify: `src/lib/components/TimelineEditor.svelte`
- Modify: `src/lib/components/TimelineEditor.test.ts`

**Step 1: 写边界预览失败测试**

断言拖动中不派发 resize，松手只派发一次。

**Step 2: 写边界约束失败测试**

覆盖零点、源时长、相反边界和逐帧吸附。

**Step 3: 写取消失败测试**

取消后恢复原始值，不提交命令。

**Step 4: 写边缘自动平移失败测试**

使用 fake timers / animation frame mock，验证启动、速度、停止和边界。

**Step 5: 实现并运行测试**

Expected: PASS。

**Step 6: Commit**

```sh
git add src/lib/components/TimelineEditor.svelte src/lib/components/TimelineEditor.test.ts
git commit -m "feat: refine timeline interval boundaries"
```

### Task 7：实现播放器预览 Seek 合并

**Files:**

- Modify: `src/lib/player/MediaPlayerAdapter.ts`
- Modify: `src/lib/player/HtmlVideoAdapter.ts`
- Modify: `src/lib/components/PlayerPane.svelte`
- Create: `src/lib/components/PlayerPane.test.ts`

**Step 1: 写 preview Seek 合并失败测试**

同一动画帧调用多次，仅最后一个目标进入 Adapter。

**Step 2: 写精确 Seek 失败测试**

覆盖：

- 已在目标时间立即完成。
- `seeked` 成功。
- error 拒绝。
- 5 秒超时拒绝。
- 所有 listener 和 timer 被清理。

**Step 3: 扩展 Adapter 接口并实现**

预览方法不等待事件，精确方法有界等待。

**Step 4: 运行测试**

```sh
pnpm exec vitest run src/lib/components/PlayerPane.test.ts
```

Expected: PASS。

**Step 5: Commit**

```sh
git add src/lib/player/MediaPlayerAdapter.ts src/lib/player/HtmlVideoAdapter.ts src/lib/components/PlayerPane.svelte src/lib/components/PlayerPane.test.ts
git commit -m "feat: coalesce timeline preview seeks"
```

### Task 8：接入 App 标记和 Scrub 流程

**Files:**

- Modify: `src/App.svelte`
- Create: `src/App.test.ts`

**Step 1: 写 Scrub App 流程失败测试**

断言：

- start 暂停并记录原播放状态。
- preview 乐观更新，旧 time 事件不覆盖。
- commit 精确定位并只保存最终位置。
- 原来播放时提交后恢复播放。

**Step 2: 写标记保存失败测试**

后端失败后 `pendingStartUs` 保留。

**Step 3: 写规范化选中失败测试**

新增或 resize 与邻区间合并后，选择新的规范化 ID。

**Step 4: 调整 runEdit 返回值**

保持现有错误、stale project 和 demo 行为。

**Step 5: 接入 TimelineEditor**

暂时保留旧组件文件，但页面不再渲染它们。

**Step 6: 运行测试和 typecheck**

```sh
pnpm exec vitest run src/App.test.ts
pnpm typecheck
```

Expected: PASS。

**Step 7: Commit**

```sh
git add src/App.svelte src/App.test.ts
git commit -m "feat: integrate unified timeline workflow"
```

### Task 9：清理旧时间轴与调整页面样式

**Files:**

- Delete: `src/lib/components/DetailTimeline.svelte`
- Delete: `src/lib/components/OverviewTimeline.svelte`
- Modify: `src/lib/components/TransportControls.svelte`
- Modify: `src/app.css`

**Step 1: 移除旧 import、组件和离散 zoom 状态**

确认 `detailWindowUs` 和旧 `zoom()` 无引用。

**Step 2: 精简播放器控制条**

移除重复标记、撤销和重做按钮。

**Step 3: 调整 Workspace 高度**

单时间轴目标高度 188–200px，检查 960×680。

**Step 4: 搜索旧样式与文案**

```sh
rg -n "DetailTimeline|OverviewTimeline|detail-block|overview-block|crosshair|精确时间轴|全局概览|detailWindowUs" src
```

Expected: 无遗留旧实现；新帮助文案中的必要说明除外。

**Step 5: 运行前端验证**

```sh
pnpm typecheck
pnpm test
```

Expected: PASS。

**Step 6: Commit**

```sh
git add src/App.svelte src/app.css src/lib/components/TransportControls.svelte
git add -u src/lib/components/DetailTimeline.svelte src/lib/components/OverviewTimeline.svelte
git commit -m "refactor: replace dual timeline layout"
```

### Task 10：更新用户和开发文档

**Files:**

- Modify: `README.md`
- Modify: `docs/SpyCut-V1-开发文档.md`
- Modify: `AGENTS.md`

**Step 1: 更新 README**

按钮优先、快捷键辅助；加入 Scrub、横移、缩放和适配全片说明。

**Step 2: 更新开发文档**

替换主界面线框、标记步骤、两级时间轴章节和前端测试要求。

**Step 3: 执行 AGENTS 自更新算法**

因为删除并替换核心时间轴组件，需要同步第 2 节代码导航；若最低验证要求发生变化，同步第 7 节。

**Step 4: 复核隐私**

不得在文档和截图中加入真实课程文件名、画面、路径或预览令牌。

**Step 5: Commit**

```sh
git add README.md docs/SpyCut-V1-开发文档.md AGENTS.md
git commit -m "docs: document unified timeline workflow"
```

### Task 11：完整验证与验收

**Files:**

- Modify when actually verified: `docs/test-reports/macos-v1.md`
- Modify on real Windows only: `docs/test-reports/windows-v1.md`

**Step 1: 运行格式和差异检查**

```sh
git diff --check
```

Expected: 无输出。

**Step 2: 运行默认验证**

```sh
pnpm check
```

Expected: 前端 typecheck、Vitest、Rust test、Clippy 和 rustfmt 全部通过。

**Step 3: 运行 macOS 实际界面**

使用非私人 demo session 或专用测试视频，验证本文第 14 节。开发服务器和 Tauri 进程必须使用有限等待并在验收后正常退出。

**Step 4: 检查残留进程**

确认无 SpyCut、FFmpeg、FFprobe 或开发服务器进程遗留。

**Step 5: Windows 验收**

只能在真实 Windows 10/11 + WebView2 上把鼠标滚轮、触控板、Pointer Capture 和 HEVC 预览写成已通过；交叉构建不替代实机。

**Step 6: 更新验收报告并 Commit**

只记录实际执行的结果和平台缺口。

## 13. 自动测试矩阵

| 范围 | 测试 |
|---|---|
| 坐标 | 左端、中心、右端、越界、零宽保护 |
| 帧吸附 | 17.12、23.976、29.97、30、59.94、60fps |
| Viewport | 完整时长、最小跨度、两端 clamp |
| 锚点缩放 | 鼠标锚点误差不超过一帧 |
| 平移 | deltaX、Shift+wheel、视频两端 |
| 导航条 | 滑块拖动、外部点击、极小视口 |
| Tick | 帧、毫秒、秒、分钟、小时 |
| Scrub | start、preview、commit、cancel |
| Pointer | capture、cancel、lost capture、销毁 |
| Seek | 合并、精确成功、失败、超时、清理 |
| 标记 | Idle、Pending、重设、取消、无效终点 |
| 保存失败 | pending 起点保留 |
| 规范化 | 新增合并、resize 合并后选中正确 ID |
| Resize | 帧吸附、边界、单次提交、取消 |
| Auto-pan | 左右触发、变速、边界停止 |
| Keyboard | 全局快捷键、slider 局部快捷键、焦点保护 |
| A11y | role、aria value、可见 label、focus ring |
| Lock | 导出期间查看可用、编辑不可用 |
| Responsive | 960px 下起止与缩放操作仍可见 |

## 14. 手工验收脚本

### 14.1 单时间轴

1. 打开三小时 demo project。
2. 确认底部只有一条带时间刻度的编辑时间轴。
3. 确认底部细条没有第二套刻度和播放头。
4. 点击“适配全片”，确认 0 到完整时长可见。
5. 拖动导航条滑块，确认只移动视口。

### 14.2 连续拖动

1. 把鼠标移入标尺，确认显示悬停时间而不是加号。
2. 从轨道 10% 位置拖到 80%。
3. 确认播放头持续跟手，没有只在松手或单击时跳转。
4. 确认视频预览更新，不出现明显 Seek 排队。
5. 松手后确认时间位置稳定，没有被旧事件拉回。
6. 播放状态下重复，确认拖动时暂停、松手后恢复。

### 14.3 缩放和平移

1. 鼠标悬停在一个明确删除边界上。
2. 使用 `Alt/Option + wheel` 放大。
3. 确认该边界仍在鼠标下方，误差不超过一帧。
4. 使用触控板横向滑动或 `Shift + wheel` 平移。
5. 使用滑杆、`− / +` 和适配全片，确认行为一致。

### 14.4 删除起止

1. 点击 `设删除起点`。
2. 确认按钮变为 `重设起点`，取消按钮出现。
3. 确认时间轴显示待完成斜纹。
4. 把播放头拖到起点之前，确认终点按钮禁用。
5. 把播放头拖到起点之后，点击 `完成删除区间`。
6. 确认区间保存、选中并出现在右侧列表。
7. 模拟保存失败，确认起点没有消失。
8. 用 `I/O/Esc` 重复，确认与按钮完全一致。

### 14.5 区间边界

1. 单击红色区间主体。
2. 确认选中并移动播放头，但区间自身未移动。
3. 拖动左、右手柄并跨过视口边缘。
4. 确认自动平移、帧吸附和松手单次保存。
5. 拖动中按 `Escape`，确认恢复原值。
6. 把区间拖到邻接另一区间，确认 Rust 合并后仍选中正确区间。
7. 确认修改后相关复核状态失效。

### 14.6 锁定与错误

1. 开始导出。
2. 确认仍可查看和定位时间，但不能创建、删除或调整区间。
3. 取消或完成导出后确认编辑恢复。
4. 模拟 Seek 超时，确认出现可处理错误而不是永久 Working。

### 14.7 响应式和平台

1. 验证 960×680。
2. 验证截图对应的宽屏比例。
3. 验证 Retina/高 DPI。
4. macOS 验证鼠标和触控板。
5. Windows 实机验证鼠标滚轮、触控板和 Pointer Capture。

## 15. 性能预算

- Pointer move 处理目标：不超过每动画帧一次视频预览提交。
- 视口和刻度计算：只与当前可视范围和可见区间数量相关。
- 不为完整视频创建逐帧 DOM。
- 不读取额外媒体数据。
- 不生成 Canvas 全片缩略图。
- 区间列表和导航条使用现有规范化区间，复杂度为 O(n)。
- 区间边界拖动中不调用 Tauri command。
- 一次 Scrub 只保存一次最终播放位置。
- 一次边界拖动只提交一次 resize。
- 所有动画帧、事件监听器和 timer 在取消、销毁和切换项目时清理。

## 16. 错误处理

| 场景 | 行为 |
|---|---|
| 精确 Seek 超时 | 停止等待，显示错误，保留可恢复 UI |
| 预览 Seek 迟到 | Scrub 期间忽略旧 time 事件 |
| Pointer cancel | 恢复拖动前状态，不提交 |
| Resize 保存失败 | 使用服务端 projection 或原值恢复 |
| 新增区间保存失败 | 保留 pending 起点 |
| stale project | 不应用旧结果，沿用现有错误处理 |
| 导出锁竞争 | 前端禁用 + 后端 workflow gate 双重保护 |
| 规范化 ID 改变 | 根据覆盖范围寻找新 ID |
| 视口宽度为 0 | 跳过坐标运算，不产生 NaN |
| duration 为 0 | 禁用时间轴操作并显示安全空状态 |

## 17. 安全与隐私

- 时间轴重构不扩大 CSP、ATS 或文件访问权限。
- 不改变 PreviewServer URL 或令牌策略。
- 不把源路径传入新组件。
- 不记录 Hover、Scrub 或删除区间对应的课程内容。
- 自动测试使用 demo session 和合成时间数据。
- 手工验收截图不得包含私人聊天、课程画面、真实绝对路径或预览令牌。
- 新增错误信息不得回显完整源文件路径。

## 18. 风险与缓解

### 18.1 WebView Pointer 行为差异

风险：WKWebView 与 WebView2 对触控板 pinch、Wheel modifier 和 Pointer Capture 存在差异。

缓解：

- 核心操作始终可通过可见滑杆和按钮完成。
- Wheel 手势只作为效率增强。
- macOS 和 Windows 分别实机验收。
- 不把未验证的 pinch 行为写成两个平台都支持。

### 18.2 HTML Video 连续 Seek 性能

风险：超高分辨率 HEVC 在拖动中无法逐帧实时解码。

缓解：

- 合并预览 Seek，只保留最新位置。
- 预览允许近似，松手执行精确 Seek。
- UI 播放头乐观跟随，不等待解码。
- 不因此生成代理文件或扩大 V1 范围。

### 18.3 App.svelte 复杂度

风险：继续把所有手势细节放入 App 会使页面协调器更难维护。

缓解：

- 纯数学进入 `lib/timeline`。
- 手势状态留在 `TimelineEditor`。
- App 只负责播放器、项目命令和跨组件生命周期。
- 不在本次顺手重构导出和复核代码。

### 18.4 导航条再次被理解为第二时间轴

风险：如果导航条加入刻度、播放头或过高样式，会重现原问题。

缓解：

- 高度不超过 14px。
- 不显示时间刻度。
- 不显示独立播放头。
- 只表现为滚动轨道和视口滑块。
- 只允许导航，不允许编辑区间。

## 19. 文档与 AGENTS.md 同步

实现后必须更新：

1. `README.md` 使用步骤和快捷键。
2. `docs/SpyCut-V1-开发文档.md` 主界面、标记流程、时间轴章节和前端测试章节。
3. `AGENTS.md` 核心组件导航，因为删除两个核心时间轴组件并新增 `TimelineEditor`。
4. macOS/Windows 验收报告，只记录实际执行结果。

不需要更新：

- 项目 schema 文档。
- Tauri command 契约。
- FFmpeg filter 和导出验收规则。
- PreviewServer 设计。

## 20. 完成定义

以下条件全部满足才可视为完成：

- [x] 主界面只有一条有刻度的时间轴。
- [x] 导航条不会被误认成第二时间轴。
- [x] 鼠标进入轨道不再显示十字加号。
- [x] 单击、连续拖动和直接抓取播放头均可定位。
- [x] 拖动中视频预览不会积压旧 Seek。
- [x] 缩放连续且锚点误差不超过一帧。
- [x] 横向平移、缩放滑杆和适配全片均可用。
- [x] 起点、终点、重设和取消按钮始终可发现。
- [x] 保存失败不会丢失待完成起点。
- [x] 区间主体不能移动。
- [x] 边界拖动逐帧吸附、可取消且只提交一次。
- [x] 规范化合并后选中正确区间。
- [x] 区间修改后复核状态失效。
- [x] 导出期间区间编辑锁定。
- [x] 键盘操作和焦点保护通过测试。
- [x] 960×680 下关键控件可见。
- [ ] macOS 鼠标和触控板完成实际验收。
- [x] Windows 未实测项明确标注，不伪报通过。
- [x] `pnpm check` 通过。
- [x] `git diff --check` 通过。
- [ ] 无遗留开发服务器、Tauri、SpyCut、FFmpeg 或 FFprobe 进程。
- [x] README、开发文档和 AGENTS.md 按真实实现同步。
- [x] 工作树没有意外生成文件或被覆盖的用户改动。

## 21. 推荐实施顺序

严格按以下顺序推进：

1. 纯数学模型。
2. 工具栏状态机。
3. 单时间轴静态结构。
4. Pointer Scrub。
5. 缩放、平移和导航条。
6. 区间边界和自动平移。
7. 播放器预览 Seek 合并。
8. App 状态与命令整合。
9. 删除旧组件和完成响应式样式。
10. 文档同步。
11. 完整自动测试。
12. macOS 实际验收。
13. Windows 实机验收。

在第 8 步完成前不删除旧组件；在第 11 步完成前不宣称功能完成；在对应平台实际执行前不更新该平台为“已通过”。

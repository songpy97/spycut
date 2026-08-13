# SpyCut 大文件预览 Range 流式服务设计

日期：2026-08-10

## 背景与目标

SpyCut V1 曾通过 Tauri `asset://` 协议把本地 MP4 交给 HTML `<video>`。在一个多 GB、数小时且 `moov` 位于文件尾部的私有 HEVC 测试样本上，WKWebView 的范围请求最终进入 `FormatError`。同一文件可被 FFmpeg 和 QuickTime 正常解码，同编码/分辨率的 `faststart` 合成样片在 SpyCut 也可预览，因此需替换预览传输层，而不是要求用户转码。

目标：

- 大小与时长不影响进程内存占用；禁止把整个文件读入内存。
- 正确支持 `HEAD`、无 Range `GET`、单 Range、多 Range、开放尾部 Range 和 suffix Range。
- 每个 Range 必须按客户端请求的完整边界返回，不擅自截短。
- 同时支持 macOS WKWebView 和 Windows WebView2。
- 仅暴露当前项目的源文件，不对 HTTP 请求接受任意文件路径。

## 架构

新增 `PreviewServer`，应用启动时绑定 `127.0.0.1:0`，由操作系统选择空闲端口。服务器只维护一条当前映射：随机 UUID 令牌、规范化源文件路径和文件长度。打开或恢复项目后，Rust 先完成媒体探测与指纹校验，再调用 `publish_source`。每次发布都生成新令牌，旧 URL 立即失效。

前端收到形如 `http://localhost:<port>/media/<token>` 的 `previewUrl`，直接设置给 `<video>`。服务实际只绑定 IPv4 loopback；使用 `localhost` 是为了同时满足 macOS 本地网络 ATS 规则和 Windows WebView2。前端不再调用 `convertFileSrc`，也不再把源文件加入 Tauri asset protocol scope。`open_source` 和 `get_session` 统一返回 `{ session, resumed, previewUrl }`，后续区间编辑只更新 session，不重置预览 URL。

## HTTP 与资源边界

- 只接受 `/media/<current-token>`；路径不匹配返回 404。
- 支持 `GET`、`HEAD` 和 CORS `OPTIONS`；其他方法返回 405。
- 响应包含 `Accept-Ranges: bytes`、正确的 `Content-Length` / `Content-Range`、`video/mp4`、`Cache-Control: no-store` 和 CORS 头。
- 无 Range 时返回 200，用固定大小缓冲区顺序流式传输。
- 单 Range 返回 206，由独立连接线程执行文件 seek，并用有界分块写入完整请求范围。
- 多 Range 返回 `multipart/byteranges`；预先计算精确 `Content-Length`，但数据仍逐段流式发送。
- 请求头限制为 16 KiB，连接并发上限 16，头部读取和单次 socket 写入都有超时。

服务仅监听 IPv4 loopback，不可从局域网访问。UUID 令牌不进入日志，HTTP 请求无法提供文件路径。CSP 的 `media-src` 仅增加 `http://localhost:*`，macOS `Info.plist` 只允许本机网络 HTTP。

## 错误处理与生命周期

绑定失败会使应用启动失败，而不是带着不可用的预览页继续。打开源文件时，如果规范化路径或 metadata 读取失败，不替换当前映射。HTTP 客户端中断只终止对应连接线程，不影响项目或导出。应用进程退出时，监听器和所有预览连接随进程一并终止。

前端仍保留 FFmpeg 独立解码诊断，但文案要区分“源视频不可解码”和“预览流加载失败”，不再笼统宣称系统没有 HEVC 解码器。

## 验收

1. 单元测试覆盖普通、开放、suffix、越界、非法和多 Range。
2. TCP 集成测试对大于 1 MiB 的单 Range 校验完整长度与首尾字节，证明不再被 Tauri 的 1000 KiB 上限截断。
3. 集成测试验证 HEAD、无 Range、多 Range、416、令牌轮换和路径隔离。
4. 运行现有 `pnpm check`、3 个 FFmpeg 重型测试、Rust/Windows `-D warnings` 和生产构建。
5. 用本地私有大文件样本直接回归：无预处理、无转码、无重封装，WKWebView 必须达到 `HaveMetadata` / `HaveEnoughData`，可播放、跳转到中后段，并且 SpyCut RSS 不随源文件大小增长。验收记录不得包含样本路径或课程内容。

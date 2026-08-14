# macOS x64 原生打包设计

## 目标与方案

SpyCut 的 GitHub Actions 当前在 `macos-15` ARM64 runner 和 `windows-2022` x64 runner 上生成发布包。新增 Intel macOS 支持时，继续使用原生 runner 构建，不在 Apple Silicon runner 上交叉编译或制作 universal binary。GitHub 官方当前提供 `macos-15-intel` 标签，因此发布矩阵新增这一项，并保留已有 `macos-15` ARM64 项。三个 package job 全部成功后，标签工作流才创建预发布 Release；手动运行仍只上传 Actions 产物。

macOS 两种架构共用 `scripts/package-macos.sh`。脚本根据 `uname -m` 将 `arm64` 映射为 Tauri sidecar target `aarch64-apple-darwin`、产物后缀 `aarch64`，将 `x86_64` 映射为 `x86_64-apple-darwin`、产物后缀 `x64`。FFmpeg/FFprobe 继续由每台原生 runner 上的 `scripts/prepare-ffmpeg-macos.sh` 构建，避免跨架构 sidecar 混用。DMG、ZIP、校验文件和 Actions artifact 都包含架构标识，防止 Release 汇总下载时同名文件覆盖。

## 验收与失败边界

打包脚本维持现有有限超时、临时签名、DMG/ZIP 完整性、应用签名、许可证和图标检查。回归测试静态验证两种 runner、两种 artifact、架构映射和架构化校验文件名。由于当前开发机是 Apple Silicon，本地只能执行脚本语法、测试和完整仓库检查；Intel DMG/ZIP 的原生构建结论必须以 `macos-15-intel` GitHub Actions 实际运行结果为准，不能提前写成已通过。

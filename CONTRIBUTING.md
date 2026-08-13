# Contributing to SpyCut

感谢你参与 SpyCut。提交改动前请先阅读根目录的 `AGENTS.md`，其中记录了不可破坏的产品约束、代码分层和验证要求。

## 开发流程

1. Fork 仓库并从 `main` 创建功能分支。
2. 使用 `pnpm install --frozen-lockfile` 安装依赖。
3. 保持源视频只读、原时间轴不可重排，并为行为变更补充测试。
4. 提交前运行 `pnpm check` 和 `git diff --check`。
5. 创建 Pull Request，说明行为变化、验证结果和未覆盖的平台测试。

不要提交视频、FFmpeg 二进制、构建目录、诊断日志、项目 sidecar，或任何包含私人课程内容和本机绝对路径的材料。

## 报告问题

请附上系统版本、SpyCut 版本、复现步骤和脱敏后的错误信息。诊断日志可能有助于定位问题，但请在上传前再次确认其中没有私人内容。

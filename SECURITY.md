# Security Policy

**English** · [简体中文](SECURITY.zh-CN.md)

## Supported versions

Security fixes currently target `main` and the latest GitHub preview release.

## Reporting a vulnerability

Please do not create a public issue for an unpatched security vulnerability. Use the repository's private **Report a vulnerability** channel and include concise reproduction steps, impact, and any suggested mitigation. The maintainer will coordinate disclosure after confirming the report.

Do not attach real course recordings, SpyCut project sidecars, preview URLs, tokens, private diagnostic content, or other user data. Use the smallest synthetic media sample that reproduces a media-processing issue.

Relevant security boundaries include source-file immutability, sidecar path derivation, loopback preview token isolation, FFmpeg/FFprobe process supervision, recovery-file scoping, transactional export, CSP/ATS configuration, and diagnostic redaction.

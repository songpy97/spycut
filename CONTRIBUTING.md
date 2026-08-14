# Contributing to SpyCut

**English** · [简体中文](CONTRIBUTING.zh-CN.md)

Thank you for helping make long recordings safer and easier to publish. SpyCut welcomes focused bug fixes, reproducible media edge cases, documentation, translations, tests, and improvements that preserve its delete-only editing model.

Before changing behavior, read [AGENTS.md](AGENTS.md). It documents the product invariants, architecture boundaries, security rules, and minimum verification expected in this repository.

## Good first contributions

- Improve English or Simplified Chinese documentation.
- Add tests for timeline, interval, persistence, preview Range, or export edge cases.
- Report a reproducible H.264/H.265 MP4 compatibility issue using synthetic media.
- Improve accessibility, keyboard navigation, or clear error messages.
- Help design a maintainable application localization layer.

Large changes to the supported media scope, editing model, persistence, preview transport, or export strategy should start with an issue or design discussion.

## Development workflow

1. Fork the repository and create a branch from `main`.
2. Install dependencies with `pnpm install --frozen-lockfile`.
3. Keep source videos read-only and the original timeline immutable.
4. Add the smallest test that reproduces a bug before fixing it.
5. Run the relevant checks, including `pnpm check` and `git diff --check` before opening a pull request.
6. In the pull request, describe behavior changes, verification performed, and platform testing that remains unverified.

```sh
scripts/check-env.sh
pnpm install --frozen-lockfile
pnpm check
git diff --check
```

Do not weaken product or security constraints merely to make a test pass. Keep changes small and avoid unrelated refactors.

## Media and privacy

Do not commit or upload:

- real course recordings or licensed media;
- FFmpeg binaries or large media fixtures;
- SpyCut project sidecars;
- diagnostic logs with private data;
- preview URLs or tokens;
- local absolute paths;
- build directories such as `dist/`, `node_modules/`, or `src-tauri/target/`.

Use the smallest synthetic sample that reproduces the problem. Describe media characteristics such as codec, frame rate, duration, stream layout, and bit depth without including private filenames or content.

## Pull requests

Pull requests should explain:

- the user-facing problem;
- why the change fits SpyCut's focused editing model;
- the implementation boundary that changed;
- tests and manual checks completed;
- macOS or Windows validation that was not performed.

Changes to Rust fields serialized as camelCase must update the matching TypeScript contracts and API boundary in the same pull request.

## Reporting issues

Use the repository's structured issue forms. Include your operating system, SpyCut version, concise reproduction steps, and sanitized error information. You may reference a diagnostic log, but inspect it again before sharing.

For vulnerabilities, do not open a public issue. Follow [SECURITY.md](SECURITY.md) and use GitHub's private reporting channel.

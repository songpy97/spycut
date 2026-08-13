#!/usr/bin/env bash
set -eu

printf '%s\n' \
  '[package-all] This cross-platform packaging path has been retired.' \
  '[package-all] Run scripts/package-macos.sh for macOS artifacts.' \
  '[package-all] Build Windows setup.exe on native Windows with scripts/package-windows.ps1 or the GitHub package workflow.' >&2
exit 2

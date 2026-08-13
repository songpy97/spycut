#!/bin/sh
set -eu

if [ "$(uname -s)" != "Darwin" ]; then
  printf '%s\n' 'This bootstrap script is for macOS only.' >&2
  exit 1
fi

if ! xcode-select -p >/dev/null 2>&1; then
  printf '%s\n' 'Xcode Command Line Tools are required.' >&2
  exit 1
fi

if ! command -v rustup >/dev/null 2>&1; then
  printf '%s\n' 'Rust is missing. Install it from https://rustup.rs, then rerun.' >&2
  exit 1
fi

rustup toolchain install 1.97.1 --profile minimal --component rustfmt --component clippy
rustup override set 1.97.1
pnpm install --frozen-lockfile=false
scripts/check-env.sh

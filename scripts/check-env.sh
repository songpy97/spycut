#!/bin/sh
set -eu

spycut_failed=0

check_command() {
  spycut_name="$1"
  if command -v "$spycut_name" >/dev/null 2>&1; then
    printf 'PASS  %-10s %s\n' "$spycut_name" "$(command -v "$spycut_name")"
  else
    printf 'FAIL  %-10s missing\n' "$spycut_name"
    spycut_failed=1
  fi
}

check_command node
check_command pnpm
check_command ffmpeg
check_command ffprobe
check_command rustc
check_command cargo

spycut_free_kib=$(df -Pk . | awk 'NR==2 {print $4}')
if [ "$spycut_free_kib" -lt 31457280 ]; then
  printf 'WARN  disk       less than 30 GiB free\n'
else
  printf 'PASS  disk       at least 30 GiB free\n'
fi

exit "$spycut_failed"


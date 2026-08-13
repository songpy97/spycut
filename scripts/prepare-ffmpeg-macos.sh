#!/bin/sh
set -eu

if [ "$(uname -s)" != "Darwin" ]; then
  printf '%s\n' 'This script only prepares the macOS FFmpeg sidecars.' >&2
  exit 1
fi

spycut_version='8.0.1'
spycut_sha256='679aa13a19415d5ddab91e580084e3ab20c963c8240001e5cbb955a97bdd81b1'
spycut_url="https://codeload.github.com/FFmpeg/FFmpeg/tar.gz/refs/tags/n${spycut_version}"
spycut_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
spycut_temp=$(mktemp -d "${TMPDIR:-/tmp}/spycut-ffmpeg.XXXXXX")
trap 'rm -rf "$spycut_temp"' EXIT HUP INT TERM

printf '%s\n' "Downloading pinned FFmpeg ${spycut_version} source..."
curl --fail --location --retry 3 --connect-timeout 20 --max-time 180 \
  --output "$spycut_temp/ffmpeg.tar.gz" "$spycut_url"
printf '%s  %s\n' "$spycut_sha256" "$spycut_temp/ffmpeg.tar.gz" | LC_ALL=C shasum -a 256 -c -
LC_ALL=C tar -xf "$spycut_temp/ffmpeg.tar.gz" -C "$spycut_temp"

spycut_source="$spycut_temp/FFmpeg-n${spycut_version}"
spycut_prefix="$spycut_temp/install"
spycut_configure_flags="--prefix=$spycut_prefix --disable-gpl --disable-nonfree --disable-doc --disable-debug --disable-network --disable-autodetect --disable-ffplay --enable-static --disable-shared --enable-videotoolbox --enable-audiotoolbox --enable-pthreads"

printf '%s\n' 'Configuring LGPL-only FFmpeg sidecars...'
(cd "$spycut_source" && ./configure $spycut_configure_flags)
spycut_jobs=$(sysctl -n hw.logicalcpu 2>/dev/null || printf '4')
printf '%s\n' "Building FFmpeg with ${spycut_jobs} jobs..."
(cd "$spycut_source" && make -j "$spycut_jobs" && make install)

case "$(uname -m)" in
  arm64) spycut_target='aarch64-apple-darwin' ;;
  x86_64) spycut_target='x86_64-apple-darwin' ;;
  *) printf '%s\n' 'Unsupported macOS architecture.' >&2; exit 1 ;;
esac

mkdir -p "$spycut_root/src-tauri/binaries" "$spycut_root/third-party/ffmpeg"
cp "$spycut_prefix/bin/ffmpeg" "$spycut_root/src-tauri/binaries/ffmpeg-${spycut_target}"
cp "$spycut_prefix/bin/ffprobe" "$spycut_root/src-tauri/binaries/ffprobe-${spycut_target}"
cp "$spycut_source/COPYING.LGPLv2.1" "$spycut_root/third-party/ffmpeg/COPYING.LGPLv2.1"
chmod +x "$spycut_root/src-tauri/binaries/ffmpeg-${spycut_target}" "$spycut_root/src-tauri/binaries/ffprobe-${spycut_target}"

if "$spycut_root/src-tauri/binaries/ffmpeg-${spycut_target}" -version | head -1 | grep -qi gpl; then
  printf '%s\n' 'Refusing a sidecar that reports GPL configuration.' >&2
  exit 1
fi

printf '%s\n' "Prepared audited sidecars for ${spycut_target}."
printf '%s\n' "Configure: $spycut_configure_flags"

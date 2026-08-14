#!/usr/bin/env bash
# Build and verify native macOS packages from this checkout.
set -Eeuo pipefail

export PATH="${HOME}/.cargo/bin:${HOME}/.local/bin:/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:${PATH}"

readonly script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
readonly repo_root="$(CDPATH= cd -- "$script_dir/.." && pwd)"
readonly app_version="$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["version"])' "$repo_root/src-tauri/tauri.conf.json")"
mac_target=''
mac_arch=''
mac_app=''
mac_dmg=''
mac_zip=''
checksums_file=''
pnpm_command=()

log() { printf '%s\n' "[package-macos] $*"; }
die() { printf '%s\n' "[package-macos] error: $*" >&2; exit 1; }

run_with_timeout() {
  local seconds="$1"
  shift
  python3 -c 'import subprocess, sys; subprocess.run(sys.argv[2:], timeout=int(sys.argv[1]), check=True)' "$seconds" "$@"
}

require_command() { command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"; }

resolve_pnpm() {
  if command -v pnpm >/dev/null 2>&1; then
    pnpm_command=(pnpm)
  else
    require_command npm
    pnpm_command=(npm exec --yes --package=pnpm@11.16.0 -- pnpm)
    log 'pnpm is not installed globally; using pinned pnpm@11.16.0 through npm.'
  fi
}

main() {
  local stage_dir=''
  local verify_dir=''
  [[ "$(uname -s)" == 'Darwin' ]] || die 'this script must run on macOS'
  case "$(uname -m)" in
    arm64)
      mac_target='aarch64-apple-darwin'
      mac_arch='aarch64'
      ;;
    x86_64)
      mac_target='x86_64-apple-darwin'
      mac_arch='x64'
      ;;
    *) die 'unsupported macOS architecture' ;;
  esac
  mac_app="$repo_root/src-tauri/target/release/bundle/macos/SpyCut.app"
  mac_dmg="$repo_root/src-tauri/target/release/bundle/dmg/SpyCut_${app_version}_${mac_arch}.dmg"
  mac_zip="$repo_root/src-tauri/target/release/bundle/macos/SpyCut_${app_version}_${mac_arch}.zip"
  checksums_file="$repo_root/src-tauri/target/release/bundle/macos/SpyCut_${app_version}_${mac_arch}_checksums.txt"
  cd "$repo_root"
  for command in python3 npm codesign ditto hdiutil plutil unzip shasum; do require_command "$command"; done
  resolve_pnpm

  if [[ ! -x "$repo_root/src-tauri/binaries/ffmpeg-$mac_target" || ! -x "$repo_root/src-tauri/binaries/ffprobe-$mac_target" ]]; then
    log 'Preparing macOS FFmpeg sidecars.'
    run_with_timeout 3600 "$repo_root/scripts/prepare-ffmpeg-macos.sh"
  fi

  log 'Running the release test suite.'
  run_with_timeout 1800 "${pnpm_command[@]}" check
  log 'Building the macOS application bundle.'
  run_with_timeout 1800 "${pnpm_command[@]}" tauri build --config src-tauri/tauri.release.conf.json --bundles app
  [[ -d "$mac_app" ]] || die 'macOS application bundle was not created'
  [[ "$(plutil -extract CFBundleIconFile raw "$mac_app/Contents/Info.plist")" == 'icon.icns' ]] || die 'macOS application bundle does not declare icon.icns'
  [[ -f "$mac_app/Contents/Resources/icon.icns" ]] || die 'macOS application bundle is missing icon.icns'

  codesign --force --sign - "$mac_app/Contents/MacOS/ffmpeg"
  codesign --force --sign - "$mac_app/Contents/MacOS/ffprobe"
  codesign --force --sign - "$mac_app/Contents/MacOS/spycut"
  codesign --force --deep --sign - "$mac_app"
  codesign --verify --deep --strict --verbose=2 "$mac_app"

  # Building only the .app does not create Tauri's DMG output directory on a
  # clean runner. Ensure both package destinations exist before writing them.
  mkdir -p "$(dirname "$mac_dmg")" "$(dirname "$mac_zip")"
  ditto -c -k --sequesterRsrc --keepParent "$mac_app" "$mac_zip"
  stage_dir="$(mktemp -d "${TMPDIR:-/tmp}/spycut-dmg-stage.XXXXXX")"
  trap '[[ -n "${stage_dir:-}" && -d "${stage_dir:-}" ]] && rm -rf -- "${stage_dir}"; if [[ -n "${verify_dir:-}" && -d "${verify_dir:-}" ]]; then hdiutil detach "${verify_dir}" >/dev/null 2>&1 || true; fi' EXIT
  ditto "$mac_app" "$stage_dir/SpyCut.app"
  ln -s /Applications "$stage_dir/Applications"
  hdiutil create -ov -volname SpyCut -srcfolder "$stage_dir" -format UDZO "$mac_dmg"
  rm -rf -- "$stage_dir"
  stage_dir=''

  run_with_timeout 600 hdiutil verify "$mac_dmg"
  run_with_timeout 600 unzip -t "$mac_zip"
  verify_dir="$(mktemp -d "${TMPDIR:-/tmp}/spycut-dmg-verify.XXXXXX")"
  hdiutil attach -readonly -nobrowse -mountpoint "$verify_dir" "$mac_dmg"
  codesign --verify --deep --strict --verbose=2 "$verify_dir/SpyCut.app"
  test -f "$verify_dir/SpyCut.app/Contents/Resources/licenses/FFmpeg-NOTICE.md"
  test -f "$verify_dir/SpyCut.app/Contents/Resources/licenses/COPYING.LGPLv2.1"
  hdiutil detach "$verify_dir"
  rmdir "$verify_dir"
  verify_dir=''

  (
    cd "$(dirname "$mac_dmg")"
    LC_ALL=C shasum -a 256 "$(basename "$mac_dmg")"
    cd "$(dirname "$mac_zip")"
    LC_ALL=C shasum -a 256 "$(basename "$mac_zip")"
  ) > "$checksums_file"
  git diff --check
  trap - EXIT
  log "macOS ${mac_arch} packages created. Build the Windows installer on Windows or through the package workflow."
  printf '%s\n' "  $mac_dmg" "  $mac_zip" "  $checksums_file"
}

main "$@"

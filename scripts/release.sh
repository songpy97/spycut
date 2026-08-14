#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
用法：
  scripts/release.sh
  scripts/release.sh major|minor|patch
  scripts/release.sh --calculate VERSION major|minor|patch
  scripts/release.sh --help

默认交互选择版本增量：
  major  大版本：1.2.3 -> 2.0.0
  minor  中版本：1.2.3 -> 1.3.0
  patch  小版本：1.2.3 -> 1.2.4

脚本会同步版本、运行 pnpm check、提交、创建 v* 标签，并原子推送 main 和标签。
未安装全局 pnpm 时，会通过 Corepack 或 npm 临时使用仓库固定的 pnpm 版本。
标签会触发仓库现有的 GitHub Actions 预发布流程。
EOF
}

die() {
  echo "错误：$*" >&2
  exit 1
}

calculate_next_version() {
  local current_version="$1"
  local increment="$2"
  if [[ ! "$current_version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
    die "版本号必须是 X.Y.Z 格式：$current_version"
  fi

  local major="${BASH_REMATCH[1]}"
  local minor="${BASH_REMATCH[2]}"
  local patch="${BASH_REMATCH[3]}"
  case "$increment" in
    major) printf '%s.0.0\n' "$((major + 1))" ;;
    minor) printf '%s.%s.0\n' "$major" "$((minor + 1))" ;;
    patch) printf '%s.%s.%s\n' "$major" "$minor" "$((patch + 1))" ;;
    *) die "未知版本增量：$increment（只能是 major、minor 或 patch）" ;;
  esac
}

confirm_release() {
  local prompt="$1"
  local answer
  printf '%s [y/N] ' "$prompt"
  if ! IFS= read -r answer; then
    return 1
  fi
  case "$answer" in
    y|Y|yes|YES|Yes) return 0 ;;
    *) return 1 ;;
  esac
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

if [[ "${1:-}" == "--calculate" ]]; then
  [[ "$#" -eq 3 ]] || die "--calculate 需要 VERSION 和 major|minor|patch"
  calculate_next_version "$2" "$3"
  exit 0
fi

[[ "$#" -le 1 ]] || die "参数过多；请运行 scripts/release.sh --help"
requested_increment="${1:-}"
if [[ -n "$requested_increment" ]]; then
  case "$requested_increment" in
    major|minor|patch) ;;
    *) die "参数只能是 major、minor 或 patch" ;;
  esac
fi

script_dir="$(CDPATH= cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(CDPATH= cd -P -- "$script_dir/.." && pwd)"
cd "$repo_root"

release_branch="main"
release_remote="origin"
workflow_path=".github/workflows/ci.yml"
export GIT_TERMINAL_PROMPT=0
version_files=(
  "package.json"
  "src-tauri/tauri.conf.json"
  "src-tauri/Cargo.toml"
  "src-tauri/Cargo.lock"
)

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "缺少必需命令：$1"
}

pnpm_command=()
resolve_pnpm() {
  if command -v pnpm >/dev/null 2>&1; then
    pnpm_command=(pnpm)
    return
  fi
  if command -v corepack >/dev/null 2>&1; then
    pnpm_command=(corepack pnpm)
    echo "未找到全局 pnpm，将通过 Corepack 使用仓库固定的 pnpm@11.16.0。"
    return
  fi
  if command -v npm >/dev/null 2>&1; then
    pnpm_command=(npm exec --yes --package=pnpm@11.16.0 -- pnpm)
    echo "未找到全局 pnpm，将通过 npm 临时使用 pnpm@11.16.0。"
    return
  fi
  die "缺少 pnpm，且无法通过 Corepack 或 npm 提供 pnpm@11.16.0"
}

for required_command in git node cargo; do
  require_command "$required_command"
done
resolve_pnpm

git_root="$(git rev-parse --show-toplevel 2>/dev/null)" || die "当前目录不是 Git 仓库"
git_root="$(CDPATH= cd -P -- "$git_root" && pwd)"
[[ "$git_root" == "$repo_root" ]] || die "请从 SpyCut 仓库运行这个脚本"
[[ -f "$workflow_path" ]] || die "缺少 $workflow_path"
grep -Fq -- '- "v*"' "$workflow_path" || die "GitHub Actions 未配置 v* 标签触发"
grep -Fq -- 'gh release create' "$workflow_path" || die "GitHub Actions 未配置 Release 创建步骤"

current_branch="$(git branch --show-current)"
[[ "$current_branch" == "$release_branch" ]] || die "发布只能从 $release_branch 分支执行，当前是 ${current_branch:-detached HEAD}"
git remote get-url "$release_remote" >/dev/null 2>&1 || die "缺少 Git remote：$release_remote"
[[ -z "$(git diff --name-only --diff-filter=U)" ]] || die "工作区存在未解决的合并冲突"
for operation_ref in MERGE_HEAD CHERRY_PICK_HEAD REVERT_HEAD; do
  if git rev-parse -q --verify "$operation_ref" >/dev/null 2>&1; then
    die "检测到未完成的 Git 操作：$operation_ref"
  fi
done
for operation_dir in rebase-merge rebase-apply; do
  if [[ -d "$(git rev-parse --git-path "$operation_dir")" ]]; then
    die "检测到未完成的 Git rebase"
  fi
done

read_json_version() {
  node -e '
    const fs = require("node:fs");
    const value = JSON.parse(fs.readFileSync(process.argv[1], "utf8")).version;
    if (typeof value !== "string") process.exit(2);
    process.stdout.write(value);
  ' "$1"
}

read_cargo_version() {
  awk '
    $0 == "[package]" { in_package = 1; next }
    in_package && /^\[/ { exit }
    in_package && /^version = "/ {
      value = $0
      sub(/^version = "/, "", value)
      sub(/"$/, "", value)
      print value
      exit
    }
  ' src-tauri/Cargo.toml
}

read_lock_version() {
  node -e '
    const fs = require("node:fs");
    const text = fs.readFileSync("src-tauri/Cargo.lock", "utf8");
    const match = text.match(/\[\[package\]\]\r?\nname = "spycut"\r?\nversion = "([^"]+)"/);
    if (!match) process.exit(2);
    process.stdout.write(match[1]);
  '
}

read_synchronized_version() {
  local package_version
  local tauri_version
  local cargo_version
  local lock_version
  package_version="$(read_json_version package.json)"
  tauri_version="$(read_json_version src-tauri/tauri.conf.json)"
  cargo_version="$(read_cargo_version)"
  lock_version="$(read_lock_version)"
  if [[ "$package_version" != "$tauri_version" || "$package_version" != "$cargo_version" || "$package_version" != "$lock_version" ]]; then
    die "版本不一致：package=$package_version tauri=$tauri_version cargo=$cargo_version lock=$lock_version"
  fi
  printf '%s\n' "$package_version"
}

remote_tag_exists() {
  local release_tag="$1"
  local output
  if ! output="$(git ls-remote --tags "$release_remote" "refs/tags/$release_tag" "refs/tags/$release_tag^{}")"; then
    die "无法检查远端标签 $release_tag"
  fi
  [[ -n "$output" ]]
}

assert_branch_not_behind() {
  local remote_ref="refs/remotes/$release_remote/$release_branch"
  git rev-parse -q --verify "$remote_ref" >/dev/null 2>&1 || die "远端分支不存在：$release_remote/$release_branch"
  git merge-base --is-ancestor "$remote_ref" HEAD || die "本地 $release_branch 落后或已与远端分叉，请先同步后再发布"
}

echo "正在同步 $release_remote/$release_branch 和远端标签…"
git fetch "$release_remote" "$release_branch" --tags --prune
assert_branch_not_behind

current_version="$(read_synchronized_version)"
current_release_tag="v$current_version"

if git show-ref --verify --quiet "refs/tags/$current_release_tag" && ! remote_tag_exists "$current_release_tag"; then
  tag_commit="$(git rev-list -n 1 "$current_release_tag")"
  head_commit="$(git rev-parse HEAD)"
  if [[ "$tag_commit" == "$head_commit" ]]; then
    [[ -z "$(git status --porcelain)" ]] || die "发现未推送的 $current_release_tag，但工作区不干净；请先处理改动"
    echo "发现本地 release commit 和标签 $current_release_tag 尚未推送。"
    if confirm_release "是否重试原子推送 $release_branch 和 $current_release_tag？"; then
      git push --atomic "$release_remote" "$release_branch" "refs/tags/$current_release_tag"
      echo "已推送 $current_release_tag，GitHub Actions 将开始构建 Release。"
    else
      echo "已取消，没有修改 Git 状态。"
    fi
    exit 0
  fi
fi

if [[ -z "$requested_increment" ]]; then
  echo
  echo "当前版本：$current_version"
  echo "请选择要增加的版本级别："
  echo "  1) major  大版本：$current_version -> $(calculate_next_version "$current_version" major)"
  echo "  2) minor  中版本：$current_version -> $(calculate_next_version "$current_version" minor)"
  echo "  3) patch  小版本：$current_version -> $(calculate_next_version "$current_version" patch)"
  echo "  q) 取消"
  while true; do
    printf '请输入 1、2、3 或 q：'
    if ! IFS= read -r choice; then
      die "没有读取到版本选择"
    fi
    case "$choice" in
      1|major) requested_increment="major"; break ;;
      2|minor) requested_increment="minor"; break ;;
      3|patch) requested_increment="patch"; break ;;
      q|Q) echo "已取消。"; exit 0 ;;
      *) echo "无效选择，请重试。" ;;
    esac
  done
fi

next_version="$(calculate_next_version "$current_version" "$requested_increment")"
release_tag="v$next_version"
if git show-ref --verify --quiet "refs/tags/$release_tag"; then
  die "本地标签已经存在：$release_tag"
fi
if remote_tag_exists "$release_tag"; then
  die "远端标签已经存在：$release_tag"
fi

echo
echo "版本计划：$current_version -> $next_version ($requested_increment)"
if grep -Fq -- '--prerelease' "$workflow_path"; then
  echo "Release 类型：GitHub prerelease（由当前 workflow 决定）"
else
  echo "Release 类型：正式 GitHub Release"
fi
echo "当前工作区的所有改动都会进入 release commit："
git status --short
if ! confirm_release "确认同步版本并运行完整发布检查吗？"; then
  echo "已取消，没有修改版本文件。"
  exit 0
fi

backup_dir="$(mktemp -d "${TMPDIR:-/tmp}/spycut-release.XXXXXX")"
mkdir -p "$backup_dir/src-tauri"
cp package.json "$backup_dir/package.json"
cp src-tauri/tauri.conf.json "$backup_dir/src-tauri/tauri.conf.json"
cp src-tauri/Cargo.toml "$backup_dir/src-tauri/Cargo.toml"
cp src-tauri/Cargo.lock "$backup_dir/src-tauri/Cargo.lock"
versions_changed=0
publication_started=0

restore_version_files() {
  cp "$backup_dir/package.json" package.json
  cp "$backup_dir/src-tauri/tauri.conf.json" src-tauri/tauri.conf.json
  cp "$backup_dir/src-tauri/Cargo.toml" src-tauri/Cargo.toml
  cp "$backup_dir/src-tauri/Cargo.lock" src-tauri/Cargo.lock
}

cleanup_release() {
  local status="$?"
  if [[ "$versions_changed" -eq 1 && "$publication_started" -eq 0 ]]; then
    restore_version_files
    echo "发布未开始，已恢复脚本运行前的版本文件。" >&2
  fi
  rm -rf -- "$backup_dir"
  trap - EXIT
  exit "$status"
}
trap cleanup_release EXIT

versions_changed=1
node - "$next_version" <<'NODE'
const fs = require("node:fs");
const nextVersion = process.argv[2];

for (const path of ["package.json", "src-tauri/tauri.conf.json"]) {
  const document = JSON.parse(fs.readFileSync(path, "utf8"));
  document.version = nextVersion;
  fs.writeFileSync(path, `${JSON.stringify(document, null, 2)}\n`);
}

const cargoPath = "src-tauri/Cargo.toml";
const cargo = fs.readFileSync(cargoPath, "utf8");
const packageVersion = /^(\[package\]\r?\nname = "spycut"\r?\nversion = ")[^"]+("$)/m;
if (!packageVersion.test(cargo)) {
  throw new Error("Cannot find the SpyCut package version in src-tauri/Cargo.toml");
}
fs.writeFileSync(cargoPath, cargo.replace(packageVersion, `$1${nextVersion}$2`));
NODE

echo "正在更新 Cargo.lock…"
cargo check --manifest-path src-tauri/Cargo.toml
updated_version="$(read_synchronized_version)"
[[ "$updated_version" == "$next_version" ]] || die "版本更新后校验失败：$updated_version"

echo "正在运行完整发布检查…"
"${pnpm_command[@]}" check
git diff --check

echo
echo "发布检查已通过，最终待提交内容："
git status --short
git diff --stat
if ! confirm_release "确认创建 release commit、标签 $release_tag 并推送到 GitHub 吗？"; then
  echo "已取消发布。"
  exit 0
fi

echo "正在重新检查远端状态…"
git fetch "$release_remote" "$release_branch" --tags --prune
assert_branch_not_behind
if remote_tag_exists "$release_tag"; then
  die "检查期间远端已出现标签：$release_tag"
fi

publication_started=1
git add -A
git diff --cached --check
git diff --cached --quiet && die "没有可提交的发布改动"
git commit -m "release: $release_tag"
git tag -a "$release_tag" -m "SpyCut $release_tag"

echo "正在原子推送 $release_branch 和 $release_tag…"
if ! git push --atomic "$release_remote" "$release_branch" "refs/tags/$release_tag"; then
  echo "原子推送失败；远端不会只收到其中一部分。" >&2
  echo "本地 release commit 和标签已保留，修复连接或权限后重新运行脚本即可续推。" >&2
  exit 1
fi

remote_url="$(git remote get-url "$release_remote")"
github_url=""
case "$remote_url" in
  https://github.com/*) github_url="${remote_url%.git}" ;;
  git@github.com:*) github_url="https://github.com/${remote_url#git@github.com:}"; github_url="${github_url%.git}" ;;
esac

echo
echo "发布已触发：$release_tag"
if [[ -n "$github_url" ]]; then
  echo "Actions：$github_url/actions"
  echo "Release：$github_url/releases/tag/$release_tag"
else
  echo "请在 GitHub Actions 中查看构建进度。"
fi

#!/usr/bin/env bash
# 公民链产品工具准备：使用调用环境中的标准工具和产品锁文件。
# 必须由 run.sh source，使产品路径和包管理器设置留在当前进程。
set -euo pipefail

PREPARE_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CITIZENCHAIN_ROOT="$(cd "$PREPARE_SCRIPT_DIR/.." && pwd)"
NODE_FRONTEND_SOURCE="$CITIZENCHAIN_ROOT/node/frontend"
ONCHINA_FRONTEND_SOURCE="$CITIZENCHAIN_ROOT/onchina/frontend"

CITIZENCHAIN_WORK_DIR="${CITIZENCHAIN_WORK_DIR:-${TMPDIR:-/tmp}/citizenchain/work}"
CITIZENCHAIN_DEPENDENCY_DIR="${CITIZENCHAIN_DEPENDENCY_DIR:-$CITIZENCHAIN_WORK_DIR/dependencies}"
python3 - "$CITIZENCHAIN_ROOT" "$CITIZENCHAIN_WORK_DIR" "$CITIZENCHAIN_DEPENDENCY_DIR" "${CARGO_TARGET_DIR:-$CITIZENCHAIN_WORK_DIR/cargo-target}" <<'CHECK_WORK'
from pathlib import Path
import sys
source = Path(sys.argv[1]).resolve()
for value in sys.argv[2:]:
    raw = Path(value)
    target = raw.resolve()
    if not raw.is_absolute() or target == source or source in target.parents:
        raise SystemExit(f'CitizenChain可写目录必须是源码外绝对路径：{value}')
CHECK_WORK
export npm_config_cache="${npm_config_cache:-$CITIZENCHAIN_DEPENDENCY_DIR/npm}"
mkdir -p "$npm_config_cache" "$CITIZENCHAIN_WORK_DIR/source"
export npm_config_audit=false npm_config_fund=false

case "$(uname -s)/$(uname -m)" in
  Darwin/arm64) CITIZENCHAIN_PROTOC_PLATFORM=macos ;;
  Linux/aarch64) CITIZENCHAIN_PROTOC_PLATFORM=linux-arm ;;
  Linux/x86_64) CITIZENCHAIN_PROTOC_PLATFORM=linux-amd ;;
  *) echo "CitizenChain protoc不支持当前宿主：$(uname -s)/$(uname -m)" >&2; exit 1 ;;
esac
# protoc由CitizenChain自己的锁定声明取得；开发者本机与CI使用同一官方版本和摘要。
PROTOC="$(node "$CITIZENCHAIN_ROOT/scripts/dependencies.mjs" prepare protoc \
  "$CITIZENCHAIN_PROTOC_PLATFORM" "$CITIZENCHAIN_DEPENDENCY_DIR/protoc/$CITIZENCHAIN_PROTOC_PLATFORM")"
[[ "$("$PROTOC" --version)" == 'libprotoc 35.0' ]] \
  || { echo 'CitizenChain protoc 35.0验真失败' >&2; exit 1; }
export PROTOC

prepare_node_project() {
  local source="$1" relative="$2" destination="$CITIZENCHAIN_WORK_DIR/source/$relative"
  [[ -f "$source/package.json" && -f "$source/package-lock.json" ]] \
    || { echo "公民链Node工程缺少锁文件：$source" >&2; return 1; }
  # 每次只重建当前产品工作根中的准确工程副本，保留包管理器缓存和其它任务目录。
  rm -rf -- "$destination"
  node - "$source" "$destination" <<'COPY_PROJECT'
const fs = require('node:fs');
const path = require('node:path');
const [source, destination] = process.argv.slice(2);
fs.cpSync(source, destination, {
  recursive: true,
  errorOnExist: true,
  force: false,
  filter: (entry) => !['node_modules', 'dist'].includes(path.basename(entry)),
});
COPY_PROJECT
  local -a npm_args=(ci)
  case "${CITIZENCHAIN_OFFLINE:-false}" in
    true) npm_args+=(--offline) ;;
    false) ;;
    *) echo 'CITIZENCHAIN_OFFLINE只接受true或false' >&2; return 1 ;;
  esac
  (cd "$destination" && npm "${npm_args[@]}")
  BUILD_NODE_PROJECT="$destination"
}

echo '==> 准备产品依赖：citizenchain/crates/scanner-react'
prepare_node_project "$CITIZENCHAIN_ROOT/crates/scanner-react" citizenchain/crates/scanner-react
SCANNER_REACT_PROJECT="$BUILD_NODE_PROJECT"
echo '==> 准备产品依赖：citizenchain/node/frontend'
prepare_node_project "$NODE_FRONTEND_SOURCE" citizenchain/node/frontend
NODE_FRONTEND_PROJECT="$BUILD_NODE_PROJECT"
echo '==> 准备产品依赖：citizenchain/onchina/frontend'
prepare_node_project "$ONCHINA_FRONTEND_SOURCE" citizenchain/onchina/frontend
ONCHINA_FRONTEND_PROJECT="$BUILD_NODE_PROJECT"

# Node桌面前端的prebuild只在工作副本内生成内置白皮书；补齐其只读输入的原目录结构。
mkdir -p "$CITIZENCHAIN_WORK_DIR/source/citizenchain/scripts"
cp "$CITIZENCHAIN_ROOT/scripts/generate-local-docs.mjs" \
  "$CITIZENCHAIN_WORK_DIR/source/citizenchain/scripts/generate-local-docs.mjs"
# 生成器与产品唯一依赖声明作为实际脚本输入进入本輪工作工程；官网原件由准备器按Git声明取得。
cp "$CITIZENCHAIN_ROOT/scripts/dependencies.mjs" "$CITIZENCHAIN_WORK_DIR/source/citizenchain/scripts/dependencies.mjs"
cp "$CITIZENCHAIN_ROOT/scripts/dependencies.json" "$CITIZENCHAIN_WORK_DIR/source/citizenchain/scripts/dependencies.json"
export SCANNER_REACT_PROJECT NODE_FRONTEND_PROJECT ONCHINA_FRONTEND_PROJECT

echo '==> 公民链产品工具和依赖已就绪'

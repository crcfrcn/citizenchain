#!/usr/bin/env bash
# 公民链非macOS本机Build保留既有源码编译矩阵，输出必须在本产品target平台目录。
set -euo pipefail
[[ $# -eq 2 && "$2" == /* ]] || { echo '用法：build-local.sh <windows|linux-arm|linux-amd> <work>' >&2; exit 2; }
case "$1" in windows|linux-arm|linux-amd) ;; *) echo '公民链Build平台无效' >&2; exit 2;; esac
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
work="$2"
[[ "$work" == "$root/target/$1/"* && -x "${CARGO:?缺少Cargo入口}" && -x "${RUSTC:?缺少Rustc入口}" ]] \
  || { echo '编译工具或本产品target输出边界无效' >&2; exit 1; }
export CARGO_TARGET_DIR="$work/cargo-target"
unset WASM_FILE
"$CARGO" check --manifest-path "$root/Cargo.toml" --locked --release -p node --no-default-features --features std --all-targets
"$CARGO" test --manifest-path "$root/Cargo.toml" --locked --release -p node --no-default-features --features std --no-run

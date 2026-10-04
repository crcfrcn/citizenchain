#!/usr/bin/env bash
# Card 05 打包前置(macOS / Linux):把 onchina 二进制 + 前端产物 + china.sqlite +
# PostgreSQL官方二进制组装到产品源码外资源目录。冻结plain chainspec已由
# node 二进制 include_bytes! 内嵌；安装包不复制 258 MB 创世 RocksDB，首启按同一
# chainspec 本地物化并由节点守卫校验块 0。之后在 node/ 跑 `npm run tauri build`。
#
# 用法:
#   export CITIZENCHAIN_PG_DIST=<postgresql.org 官方二进制解压目录(含 bin/lib/share)>
#   citizenchain/scripts/prepack.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"   # citizenchain/scripts
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"          # citizenchain/
HERE="$ROOT/node"                             # citizenchain/node
CITIZENCHAIN_WORK_DIR="${CITIZENCHAIN_PREPACK_WORK_DIR:-${TMPDIR:-/tmp}/citizenchain/prepack}"
CITIZENCHAIN_DEPENDENCY_DIR="${CITIZENCHAIN_DEPENDENCY_DIR:-$CITIZENCHAIN_WORK_DIR/dependencies}"
export CITIZENCHAIN_WORK_DIR CITIZENCHAIN_DEPENDENCY_DIR
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$CITIZENCHAIN_WORK_DIR/cargo-target}"
PACKAGE_RESOURCES="${CITIZENCHAIN_PACKAGE_RESOURCES_DIR:-$CITIZENCHAIN_WORK_DIR/resources}"
ONCHINA_BUILD_DIST="$CITIZENCHAIN_WORK_DIR/onchina-frontend/dist"
source "$SCRIPT_DIR/prepare-toolchain.sh"
case "$(uname -s)" in
  Darwin) OS=macos ;;
  Linux) OS=linux ;;
  *) OS=linux ;;
esac

echo "[prepack] build onchina (release)"
( cd "$ROOT" && cargo build -p onchina --release --config "$ROOT/config.toml" )

echo "[prepack] build onchina frontend"
( cd "$ONCHINA_FRONTEND_PROJECT" && ONCHINA_FRONTEND_DIST="$ONCHINA_BUILD_DIST" npm run build )

echo "[prepack] assemble node/resources"
mkdir -p "$PACKAGE_RESOURCES/onchina-bin" "$PACKAGE_RESOURCES/onchina-frontend" "$PACKAGE_RESOURCES/postgres"
# onchina 二进制随包(Tauri resources/onchina-bin),onchina_proc 从资源目录解析(见 node/src/onchina_proc)。
cp "$CARGO_TARGET_DIR/release/onchina" "$PACKAGE_RESOURCES/onchina-bin/onchina"
chmod +x "$PACKAGE_RESOURCES/onchina-bin/onchina"
rm -rf "$PACKAGE_RESOURCES/onchina-frontend/dist"
cp -R "$ONCHINA_BUILD_DIST" "$PACKAGE_RESOURCES/onchina-frontend/dist"

# PostgreSQL 官方二进制(postgresql.org):把已解压的 PG 安装目录(含 bin/lib/share)
# 指向 CITIZENCHAIN_PG_DIST,脚本拷进 resources/postgres/$OS;未提供则告警(安装包将缺内嵌 PG)。
if [ -n "${CITIZENCHAIN_PG_DIST:-}" ] && [ -d "$CITIZENCHAIN_PG_DIST/bin" ]; then
  rm -rf "$PACKAGE_RESOURCES/postgres/$OS"
  mkdir -p "$PACKAGE_RESOURCES/postgres/$OS"
  cp -R "$CITIZENCHAIN_PG_DIST/." "$PACKAGE_RESOURCES/postgres/$OS/"
  echo "[prepack] PostgreSQL 已组装($OS)"
else
  echo "[prepack][warn] 未提供 CITIZENCHAIN_PG_DIST。"
  echo "                请从 https://www.postgresql.org/download/ 取本平台官方二进制(含 bin/lib/share),"
  echo "                解压后 export CITIZENCHAIN_PG_DIST=<解压目录> 再重跑;否则安装包不含内嵌 PG。"
fi

# 中文注释：release 状态包只作为正式创世审计制品保留在 target/chainspec，不进入任一
# 平台安装包；清掉旧预打包残留，保证本机 prepack 与 GitHub CI 使用同一轻量合同。
rm -rf "$PACKAGE_RESOURCES/genesis-state"
echo "[prepack] 已确认安装包不携带 genesis-state；首启按冻结 plain chainspec 本地物化"

echo "[prepack] 完成：资源目录 $PACKAGE_RESOURCES"

#!/usr/bin/env bash
# 账户派生金标向量本机检查入口。
#
# 作用:
#   1. 默认只验证 account_derive 唯一真源；--write 才更新 canonical fixture。
#   2. 默认逐字节比较 canonical 与 citizenapp Dart 金标；--write 才复制。
#   3. `git diff --exit-code` 两份文件:若与提交版有任何差异则失败(=有人改了派生算法/常量却没提交刷新后的金标,或 Dart 副本漂移)。
#
# 用法:
#   citizenchain/scripts/sync-derive-vectors.sh           # 默认 check:只读核对金标与提交版
#   citizenchain/scripts/sync-derive-vectors.sh --write    # 显式刷新金标并保留 diff

set -euo pipefail

# 仓库根(本脚本位于 <repo>/citizenchain/scripts/)。
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
# 所有独立入口的工具临时状态归本产品target；宿主已交付的产品工作根继续归当前任务。
PRODUCT_TEMP_SCRIPT="${BASH_SOURCE[0]}"
while [[ -L "$PRODUCT_TEMP_SCRIPT" ]]; do
  PRODUCT_TEMP_LINK="$(readlink "$PRODUCT_TEMP_SCRIPT")"
  [[ "$PRODUCT_TEMP_LINK" == /* ]] || PRODUCT_TEMP_LINK="$(cd "$(dirname "$PRODUCT_TEMP_SCRIPT")" && pwd -P)/$PRODUCT_TEMP_LINK"
  PRODUCT_TEMP_SCRIPT="$PRODUCT_TEMP_LINK"
done
PRODUCT_TEMP_SOURCE="$(cd "$(dirname "$PRODUCT_TEMP_SCRIPT")/.." && pwd -P)"
PRODUCT_TARGET_TEMP_ROOT="$("${PRODUCT_NODE_BIN:-${NODE:-node}}" "$PRODUCT_TEMP_SOURCE/scripts/build.mjs" temporary-root "${PLATFORM:-${platform:-}}" 'macos')" || exit 1
if [[ -z "${PRODUCT_WORK_DIR:-}" && "${TMPDIR:-}" != "$PRODUCT_TEMP_SOURCE/target/"* ]]; then
  export TMPDIR="$PRODUCT_TARGET_TEMP_ROOT/"
fi
SHARED_WORK_DIR="${CITIZENCHAIN_VECTOR_WORK_DIR:-${TMPDIR:-$PRODUCT_TARGET_TEMP_ROOT}/citizenchain/sync-derive-vectors}"
mkdir -p "$SHARED_WORK_DIR"
export CARGO_TARGET_DIR="$SHARED_WORK_DIR/cargo"
export TMPDIR="$SHARED_WORK_DIR/"

PRIMITIVES_MANIFEST="${REPO_ROOT}/citizenchain/runtime/primitives/Cargo.toml"
CANONICAL="${REPO_ROOT}/citizenchain/runtime/primitives/tests/fixtures/account_derive_vectors.json"
DART_COPY="${REPO_ROOT}/citizenapp/test/citizen/shared/account_derive_vectors.json"

MODE="check"
if [[ "${1:-}" == "--write" ]]; then
  MODE="write"
fi

echo "[sync] 1/3 验证 account_derive canonical 金标 fixture ..."
# 默认检查不得改写 Runtime 输入；只有调用者明确选择 --write 才启用原有导出开关。
update=0
[[ "$MODE" != write ]] || update=1
ACCOUNT_DERIVE_UPDATE="$update" cargo test --config "$REPO_ROOT/citizenchain/config.toml" \
  --manifest-path "${PRIMITIVES_MANIFEST}" \
  --test account_derive_golden \
  -- --nocapture

echo "[sync] 2/3 校验 canonical 与 Dart 金标 ..."
if [[ "$MODE" == write ]]; then
  cp "${CANONICAL}" "${DART_COPY}"
else
  cmp "${CANONICAL}" "${DART_COPY}"
fi

echo "[sync] 3/3 校验两份金标与提交版一致 ..."
if [[ "${MODE}" == "write" ]]; then
  echo "[sync] --write 模式:保留改动,跳过 diff 守卫。请检查并提交:"
  echo "         ${CANONICAL}"
  echo "         ${DART_COPY}"
  git -C "${REPO_ROOT}" --no-pager diff -- "${CANONICAL}" "${DART_COPY}" || true
  exit 0
fi

# check 模式只读核对，必须与提交版逐字节一致。
if ! git -C "${REPO_ROOT}" diff --exit-code -- "${CANONICAL}" "${DART_COPY}"; then
  echo "" >&2
  echo "[sync] ✗ 金标向量与提交版不一致!" >&2
  echo "[sync]   原因:account_derive 算法/常量变了却没刷新金标,或 Dart 副本漂移。" >&2
  echo "[sync]   修复:确认算法变更后，在准确共享任务中执行 'citizenchain/scripts/sync-derive-vectors.sh --write'。" >&2
  exit 1
fi

echo "[sync] ✓ 金标向量一致(canonical + Dart 副本)。"

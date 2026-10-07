#!/usr/bin/env bash
# 本地跑第一阶段可安全生成的 pallet benchmark，生成 weights.rs。
# 需要的时候手动跑，把生成的 weights.rs 提交到仓库。
#
# 用法：
#   ./scripts/benchmark.sh          # 跑第一阶段所有可安全生成的 pallet
#   ./scripts/benchmark.sh pow_difficulty   # 只跑指定 pallet
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHAIN_ROOT="$(dirname "$SCRIPT_DIR")"

case "$(uname -s)/$(uname -m)" in
  Darwin/arm64) BENCHMARK_PLATFORM=macos ;;
  Linux/aarch64) BENCHMARK_PLATFORM=linux-arm ;;
  Linux/x86_64) BENCHMARK_PLATFORM=linux-amd ;;
  *) echo '公民链benchmark宿主未声明' >&2; exit 1 ;;
esac
CITIZENCHAIN_WORK_DIR="$CHAIN_ROOT/target/$BENCHMARK_PLATFORM/test/benchmark"
export CITIZENCHAIN_WORK_DIR
export CARGO_TARGET_DIR="$CITIZENCHAIN_WORK_DIR/cargo-target"
# benchmark 必须基于当前源码生成 weights，不从 GitHub CI 下载 wasm。
# runtime 正式升级走链上 setCode，CI wasm 只供链上升级流程显式使用。
unset WASM_FILE
unset SKIP_WASM_BUILD
export WASM_BUILD_FROM_SOURCE=1
export FORCE_WASM_BUILD="benchmark-$(date +%s)"
echo "==> 使用本地源码构建 benchmark runtime，不下载 GitHub CI WASM..."

# ── 1. 清除 runtime 缓存，用当前源码编译 ──
echo "==> 清除 runtime 缓存..."
find "$CARGO_TARGET_DIR" -maxdepth 3 -type d -name "citizenchain-*" -path "*/build/*" -exec rm -rf {} + 2>/dev/null || true
find "$CARGO_TARGET_DIR" -maxdepth 2 -type d -name "citizenchain" -path "*/wbuild/*" -exec rm -rf {} + 2>/dev/null || true
echo "    已清除"

# ── 2. 编译带 benchmark feature 的 node ──
# 编译前端只在本轮target工程副本准备；冻结源码继续只读。
source "$SCRIPT_DIR/prepare-toolchain.sh"
FRONTEND_DIST="$NODE_FRONTEND_PROJECT/dist"
if [ ! -d "$FRONTEND_DIST" ]; then
    npm --prefix "$NODE_FRONTEND_PROJECT" run build
fi

echo "==> 编译 benchmark node（release）..."
cd "$CHAIN_ROOT"
cargo build --release --features runtime-benchmarks --bin citizenchain --config "$CHAIN_ROOT/config.toml"
echo "    编译完成"

# 当前 runtime 的链规 preset 只在 std 节点侧提供，WASM 不能通过
# `--genesis-builder=runtime` 构造完整创世状态。基准因此从当前二进制导出一次性
# fresh spec，并用 spec-genesis 交给 benchmark externalities；退出后立即删除。
BENCHMARK_TMP_ROOT="$CHAIN_ROOT/target/$BENCHMARK_PLATFORM/test/benchmark/tmp"
mkdir -p "$BENCHMARK_TMP_ROOT"
BENCHMARK_SPEC="$(mktemp "$BENCHMARK_TMP_ROOT/citizenchain-benchmark-spec.XXXXXX")"
trap 'rm -f "$BENCHMARK_SPEC"' EXIT
"$CARGO_TARGET_DIR/release/citizenchain" export-chain-spec \
    --chain citizenchain-fresh \
    --output "$BENCHMARK_SPEC"
echo "==> 已导出当前源码 fresh spec: $BENCHMARK_SPEC"

# ── 3. 跑 benchmark ──
# 本清单只包含 benchmark 覆盖当前 WeightInfo 的 pallet。
# 以下模块当前不得自动覆盖:
# - public_manage/private_manage: benchmark 夹具没有执行完整治理外部调用，不能覆盖正式权重。
# - personal_manage / offchain_transaction:benchmark 文件为空或未挂载到 runtime registry。
# - onchain_issuance:业务仍是 stub,正式权重必须等业务实装后生成。
# - genesis_pallet:无 extrinsic,WeightInfo 为空实现。
PALLETS=(
    "provincialbank_interest:runtime/issuance/provincialbank-interest/src/weights.rs"
    "fullnode_issuance:runtime/issuance/fullnode-issuance/src/weights.rs"
    "citizen_issuance:runtime/issuance/citizen-issuance/src/weights.rs"
    "resolution_issuance:runtime/issuance/resolution-issuance/src/weights.rs"
    "citizen_identity:runtime/misc/citizen-identity/src/weights.rs"
    "pow_difficulty:runtime/misc/pow-difficulty/src/weights.rs"
    "resolution_destroy:runtime/governance/resolution-destroy/src/weights.rs"
    "grandpakey_change:runtime/governance/grandpakey-change/src/weights.rs"
    "multisig:runtime/transaction/multisig/src/weights.rs"
    "internal_vote:runtime/votingengine/internal-vote/src/weights.rs"
    "joint_vote:runtime/votingengine/joint-vote/src/weights.rs"
    "votingengine:runtime/votingengine/src/weights.rs"
    "legislation_vote:runtime/votingengine/legislation-vote/src/weights.rs"
    "election_vote:runtime/votingengine/election-vote/src/weights.rs"
    "runtime_upgrade:runtime/governance/runtime-upgrade/src/weights.rs"
)

FILTER="${1:-}"
FAILED=0
MATCHED=0

for entry in "${PALLETS[@]}"; do
    PALLET="${entry%%:*}"
    OUTPUT="${entry##*:}"

    # 如果指定了 pallet 名，只跑那一个
    if [ -n "$FILTER" ] && [ "$PALLET" != "$FILTER" ]; then
        continue
    fi
    MATCHED=1

    echo ""
    echo "══════════════════════════════════════"
    echo "▶ $PALLET"
    echo "══════════════════════════════════════"
    if "$CARGO_TARGET_DIR/release/citizenchain" benchmark pallet \
        --chain="$BENCHMARK_SPEC" \
        --genesis-builder=spec-genesis \
        --pallet="$PALLET" \
        --extrinsic='*' \
        --steps=50 \
        --repeat=20 \
        --template="$CHAIN_ROOT/scripts/benchmark-weight-template.hbs" \
        --output="$OUTPUT"; then
        echo "✓ $PALLET → $OUTPUT"
    else
        echo "✗ $PALLET 失败"
        FAILED=$((FAILED + 1))
    fi
done

echo ""
if [ -n "$FILTER" ] && [ "$MATCHED" -eq 0 ]; then
    echo "⚠ 未找到 pallet: $FILTER"
    exit 1
fi

if [ "$FAILED" -gt 0 ]; then
    echo "⚠ $FAILED 个 pallet benchmark 失败"
    exit 1
else
    echo "✓ 全部完成，weights.rs 已更新。记得提交到仓库。"
fi

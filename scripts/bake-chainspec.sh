#!/usr/bin/env bash
# 烘焙 CitizenChain 冻结 chainspec(plain 形态,ADR-031 D5)。
#
# 当前创世只直铸国家/省/市公权机构;镇级和新增机构运行期注册上链。
# 冻结 SSOT 为 plain JSON(runtime WASM + genesis patch + bootnodes)。脚本启动临时节点物化块 0,
# 同时导出用于正式创世审计的 genesis-state 链数据库包;CitizenApp/smoldot 用 stateRootHash 轻形态。
#
# 默认模式只生成预览文件到 target/build/chainspec,不覆盖冻结 SSOT。
# 正式创世必须在 GitHub WASM CI 成功后执行:
#   citizenchain/scripts/bake-chainspec.sh --finalize \
#     --wasm /path/to/citizenchain.compact.compressed.wasm \
#     --wasm-ci-run-id <RUN_ID> --wasm-ci-head-sha <HEAD_SHA>
#
# 正式模式会同步:
#   1. citizenchain/node/citizenchain.json   (节点冻结 SSOT)
#   2. citizenapp/assets/chainspec.json                        (smoldot 轻形态:stateRootHash)
#   3. citizenapp/assets/light_sync_state.json                 (smoldot checkpoint)
#   4. citizenapp/assets/public_institutions/*.json            (块 0 公权机构缓存)
#   5. citizenserve/scripts/wrangler.toml             (公开链身份派生配置)
#
# 流程:导出 plain spec → 临时节点物化创世(记录耗时)→ RPC 宪法创世检查
#       → 读块 0 头生成轻形态与 lightSyncState → 从同一块生成公权机构缓存
#       → 导出 genesis-state → 全部校验后 finalize 同步。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHAIN_ROOT="$(dirname "$SCRIPT_DIR")"
REPO_ROOT="$(dirname "$CHAIN_ROOT")"
HOST_TEMP="$(node "$CHAIN_ROOT/scripts/build.mjs" temporary-root '' macos)" || exit 1
export CARGO_TARGET_DIR="$HOST_TEMP/bake-chainspec/cargo-target"
export TMPDIR="$HOST_TEMP/bake-chainspec/tmp"
mkdir -p "$TMPDIR"
OUT="$CHAIN_ROOT/target/build/chainspec/citizenchain.json"
APP_OUT="$CHAIN_ROOT/target/build/chainspec/chainspec.app.json"
APP_LIGHT_SYNC_STATE_OUT="$CHAIN_ROOT/target/build/chainspec/light_sync_state.json"
APP_PUBLIC_INSTITUTION_OUT="$CHAIN_ROOT/target/build/chainspec/public_institutions"
CLOUDFLARE_WRANGLER_OUT="$CHAIN_ROOT/target/build/chainspec/wrangler.toml"
GENESIS_STATE_OUT="$CHAIN_ROOT/target/build/chainspec/genesis-state"
FINALIZE=0
SKIP_CHECK=0
WASM_FILE_ARG=""
WASM_CI_RUN_ID=""
WASM_CI_HEAD_SHA=""
RPC_PORT=19944
# 中文注释：临时节点必须经明确配置的可信 HTTPS 入口读取；不拼造不存在的本机 TLS 服务。
RPC_URL="${CITIZENCHAIN_RPC_URL:-}"
python3 - "$RPC_URL" <<'TLS_RPC'
import sys
from urllib.parse import urlsplit
url = urlsplit(sys.argv[1])
if url.scheme != "https" or not url.hostname or url.username or url.password or url.fragment:
    raise SystemExit("CITIZENCHAIN_RPC_URL 必须配置可信 HTTPS 入口")
TLS_RPC

validate_genesis_state_package() {
    local package_root="$1" require_release="$2" path relative
    [[ -f "$package_root/manifest.json" ]] || { echo "错误:创世状态包缺少 manifest.json:$package_root" >&2; return 1; }
    [[ -d "$package_root/chains/citizenchain/db" ]] || { echo "错误:创世状态包缺少链数据库:$package_root" >&2; return 1; }

    # 正式包不得携带临时节点生成的 TLS、network、keystore 或日志目录；只允许清单和链数据库。
    while IFS= read -r -d '' path; do
        relative="${path#"$package_root"/}"
        if [[ -L "$path" ]]; then
            echo "错误:创世状态包禁止符号链接:$relative" >&2
            return 1
        fi
        case "$relative" in
            manifest.json|chains|chains/citizenchain|chains/citizenchain/db|chains/citizenchain/db/*) ;;
            *)
                echo "错误:创世状态包包含白名单外残留:$relative" >&2
                return 1
                ;;
        esac
    done < <(find "$package_root" -mindepth 1 -print0)

    python3 - "$package_root/manifest.json" <<'PYEOF'
import json
import sys

manifest_path = sys.argv[1]
with open(manifest_path, encoding="utf-8") as f:
    manifest = json.load(f)
required = (
    "package_format", "chain_id", "genesis_hash", "state_root", "chainspec_hash",
    "runtime_wasm_hash", "light_sync_state_hash", "public_institution_root",
    "artifact_stage",
)
missing = [key for key in required if not manifest.get(key)]
if missing:
    raise SystemExit(f"创世状态包 manifest 缺少字段:{','.join(missing)}")
if manifest["package_format"] != "citizenchain-genesis-state":
    raise SystemExit("创世状态包 manifest.package_format 无效")
if manifest["chain_id"] != "citizenchain":
    raise SystemExit("创世状态包 manifest.chain_id 无效")
if manifest.get("included_paths") != ["chains/citizenchain/db"]:
    raise SystemExit("创世状态包 manifest.included_paths 必须精确等于 chains/citizenchain/db")
if manifest["artifact_stage"] not in ("preview", "release"):
    raise SystemExit("创世状态包 manifest.artifact_stage 无效")
PYEOF

    if [[ "$require_release" == "1" ]]; then
        python3 - "$package_root/manifest.json" <<'PYEOF'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as f:
    manifest = json.load(f)
if manifest.get("artifact_stage") != "release":
    raise SystemExit("正式创世状态包 artifact_stage 必须为 release")
if not str(manifest.get("runtime_wasm_ci_run_id", "")).isdigit():
    raise SystemExit("正式创世状态包 runtime_wasm_ci_run_id 无效")
head_sha = manifest.get("runtime_wasm_ci_head_sha")
if not isinstance(head_sha, str) or len(head_sha) != 40 or any(c not in "0123456789abcdef" for c in head_sha):
    raise SystemExit("正式创世状态包 runtime_wasm_ci_head_sha 无效")
PYEOF
    fi
}

usage() {
    cat <<'EOF'
Usage:
  citizenchain/scripts/bake-chainspec.sh [--out FILE] [--skip-check]
  citizenchain/scripts/bake-chainspec.sh --finalize --wasm FILE --wasm-ci-run-id ID --wasm-ci-head-sha SHA [--out FILE]

Options:
  --out FILE       生成 plain chainspec 的输出路径。默认 citizenchain/target/build/chainspec/citizenchain.json
  --genesis-state-out DIR
                   生成已物化创世链状态包的输出目录。默认 citizenchain/target/build/chainspec/genesis-state
  --wasm FILE      GitHub WASM CI 产出的 runtime wasm。正式创世必须提供
  --wasm-ci-run-id ID
                   该 WASM artifact 所属 GitHub Actions run id
  --wasm-ci-head-sha SHA
                   该 WASM artifact 所属提交 SHA
  --finalize       同步节点/App/公权机构缓存/Cloudflare 的全部冻结派生产物
  --skip-check     跳过宪法创世检查。只用于排障,正式创世不得使用
  -h, --help       显示帮助
EOF
}

while (($#)); do
    case "$1" in
        --out)
            OUT="${2:?--out 需要文件路径}"
            shift 2
            ;;
        --genesis-state-out)
            GENESIS_STATE_OUT="${2:?--genesis-state-out 需要目录路径}"
            shift 2
            ;;
        --wasm)
            WASM_FILE_ARG="${2:?--wasm 需要 wasm 文件路径}"
            shift 2
            ;;
        --wasm-ci-run-id)
            WASM_CI_RUN_ID="${2:?--wasm-ci-run-id 需要 run id}"
            shift 2
            ;;
        --wasm-ci-head-sha)
            WASM_CI_HEAD_SHA="${2:?--wasm-ci-head-sha 需要提交 SHA}"
            shift 2
            ;;
        --finalize)
            FINALIZE=1
            shift
            ;;
        --skip-check)
            SKIP_CHECK=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "未知参数: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if [[ "$FINALIZE" == "1" && -z "$WASM_FILE_ARG" ]]; then
    echo "错误: --finalize 必须同时提供 --wasm FILE,确保 :code 来自已通过 CI 的 WASM。" >&2
    exit 2
fi
if [[ "$FINALIZE" == "1" && ( -z "$WASM_CI_RUN_ID" || -z "$WASM_CI_HEAD_SHA" ) ]]; then
    echo "错误: --finalize 必须提供 --wasm-ci-run-id 与 --wasm-ci-head-sha,记录 CI artifact 来源。" >&2
    exit 2
fi
if [[ "$FINALIZE" == "1" && "$SKIP_CHECK" == "1" ]]; then
    echo "错误: 正式 --finalize 禁止 --skip-check。" >&2
    exit 2
fi
if [[ -n "$WASM_CI_RUN_ID" && ! "$WASM_CI_RUN_ID" =~ ^[0-9]+$ ]]; then
    echo "错误: --wasm-ci-run-id 必须为纯数字。" >&2
    exit 2
fi
if [[ -n "$WASM_CI_HEAD_SHA" && ! "$WASM_CI_HEAD_SHA" =~ ^[0-9a-f]{40}$ ]]; then
    echo "错误: --wasm-ci-head-sha 必须为 40 位小写十六进制提交 SHA。" >&2
    exit 2
fi

if [[ -n "$WASM_FILE_ARG" ]]; then
    if [[ ! -s "$WASM_FILE_ARG" ]]; then
        echo "错误: WASM 文件不存在或为空: $WASM_FILE_ARG" >&2
        exit 2
    fi
    WASM_FILE="$(cd "$(dirname "$WASM_FILE_ARG")" && pwd)/$(basename "$WASM_FILE_ARG")"
    export WASM_FILE
    unset WASM_BUILD_FROM_SOURCE
    echo "==> 使用指定 WASM_FILE: $WASM_FILE"
else
    export WASM_BUILD_FROM_SOURCE=1
    unset WASM_FILE
    echo "==> 未指定 --wasm,仅做本地预览:从源码构建 runtime WASM"
fi

mkdir -p "$(dirname "$OUT")" "$CHAIN_ROOT/target/build/chainspec"
TMP="$(mktemp "$CHAIN_ROOT/target/build/chainspec/.citizenchain.plain.XXXXXX.json")"
NODE_TMP_DIR="$(mktemp -d "$CHAIN_ROOT/target/build/chainspec/.bakenode.XXXXXX")"
NODE_PID=""
cleanup() {
    [[ -n "$NODE_PID" ]] && kill "$NODE_PID" 2>/dev/null || true
    rm -f "$TMP"
    rm -rf "$NODE_TMP_DIR"
}
trap cleanup EXIT

echo "==> 导出 fresh plain chainspec..."
(
    cd "$CHAIN_ROOT"
    cargo run --config "$CHAIN_ROOT/config.toml" -p node -- export-chain-spec --chain citizenchain-fresh > "$TMP"
)

rpc() {
    # RPC 轮询必须有限时；解析错误交给调用点决定是否继续等待或立即失败。
    curl -fsS --max-time 10 --proto '=https' --proto-redir '=https' -H 'content-type: application/json' \
        -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":$2}" \
        "$RPC_URL" | python3 -c '
import json
import sys

data = json.load(sys.stdin)
error = data.get("error")
if error:
    raise SystemExit(f"RPC error: {error}")
if data.get("result") is None:
    raise SystemExit("RPC result is null")
print(json.dumps(data["result"], ensure_ascii=False))
'
}

echo "==> 启动临时节点物化创世(国家/省/市公权机构,记录耗时)..."
GENESIS_T0=$(date +%s)
(
    cd "$CHAIN_ROOT"
    CITIZENCHAIN_HEADLESS=1 "$CARGO_TARGET_DIR/debug/citizenchain" --chain "$TMP" \
        --base-path "$NODE_TMP_DIR" --rpc-port "$RPC_PORT" \
        --no-mdns --no-prometheus --no-telemetry \
        >"$NODE_TMP_DIR/node.log" 2>&1
) &
NODE_PID=$!

GENESIS_HASH="null"
for _ in $(seq 1 120); do
    sleep 5
    if ! kill -0 "$NODE_PID" 2>/dev/null; then
        echo "错误: 临时节点提前退出,日志尾部:" >&2
        tail -20 "$NODE_TMP_DIR/node.log" >&2
        exit 1
    fi
    GENESIS_HASH=$(rpc chain_getBlockHash '[0]' 2>/dev/null || echo null)
    [[ "$GENESIS_HASH" != "null" && -n "$GENESIS_HASH" ]] && break
done
if [[ "$GENESIS_HASH" == "null" || -z "$GENESIS_HASH" ]]; then
    echo "错误: 10 分钟内未完成创世物化,日志尾部:" >&2
    tail -20 "$NODE_TMP_DIR/node.log" >&2
    exit 1
fi
GENESIS_SECS=$(( $(date +%s) - GENESIS_T0 ))
GENESIS_HASH_STR=$(echo "$GENESIS_HASH" | tr -d '"')
STATE_ROOT=$(rpc chain_getHeader "[$GENESIS_HASH]" | python3 -c 'import sys,json;print(json.loads(sys.stdin.read())["stateRoot"])')
echo "==> 创世物化完成: 耗时 ${GENESIS_SECS}s, genesis=$GENESIS_HASH_STR, stateRoot=$STATE_ROOT"

if [[ "$SKIP_CHECK" != "1" ]]; then
    echo "==> 检查宪法创世与冻结条件(RPC 模式)..."
    CHECK_ARGS=("$SCRIPT_DIR/check-constitution-genesis.py" --rpc "$RPC_URL" --at "$GENESIS_HASH_STR")
    if [[ -n "$WASM_FILE_ARG" ]]; then
        CHECK_ARGS+=(--expect-code-file "$WASM_FILE")
    fi
    python3 "${CHECK_ARGS[@]}"
else
    echo "==> 已跳过宪法创世检查(--skip-check)"
fi

echo "==> 生成 CitizenApp 轻形态 chainspec(stateRootHash)..."
python3 - "$TMP" "$APP_OUT" "$STATE_ROOT" <<'PYEOF'
import json, sys
plain_path, app_path, state_root = sys.argv[1], sys.argv[2], sys.argv[3]
plain = json.load(open(plain_path))
# 轻形态:去掉 runtimeGenesis(完整 state 不进 App),只留 stateRootHash;
# smoldot 据此自建创世头,校验后续区块。
app = {k: plain[k] for k in
       ("name", "id", "chainType", "bootNodes", "telemetryEndpoints",
        "protocolId", "properties", "codeSubstitutes") if k in plain}
app["genesis"] = {"stateRootHash": state_root}
json.dump(app, open(app_path, "w"), ensure_ascii=False, indent=2)
print(f"    {app_path}")
PYEOF

echo "==> 生成 CitizenApp lightSyncState checkpoint..."
LIGHT_SYNC_STATE_JSON="$(rpc sync_state_genLightSyncState '[]')"
python3 - "$APP_LIGHT_SYNC_STATE_OUT" "$LIGHT_SYNC_STATE_JSON" <<'PYEOF'
import json
import re
import sys

out_path, raw = sys.argv[1], sys.argv[2]
lss = json.loads(raw)
required = ("finalizedBlockHeader", "grandpaAuthoritySet")
missing = [key for key in required if key not in lss]
if missing:
    raise SystemExit(f"lightSyncState 缺少字段:{','.join(missing)}")
for key in required:
    value = lss[key]
    if not isinstance(value, str) or not re.fullmatch(r"0x[0-9a-fA-F]+", value):
        raise SystemExit(f"lightSyncState.{key} 必须为 0x 十六进制字符串")
with open(out_path, "w", encoding="utf-8") as f:
    json.dump(lss, f, ensure_ascii=False, indent=2)
    f.write("\n")
print(f"    {out_path}")
PYEOF

echo "==> 从同一块 0 生成 CitizenApp 公权机构缓存..."
rm -rf "$APP_PUBLIC_INSTITUTION_OUT"
node "$REPO_ROOT/citizenapp/scripts/generate_public_institution_bundle.mjs" \
    --rpc-url "$RPC_URL" \
    --at "$GENESIS_HASH_STR" \
    --chainspec "$APP_OUT" \
    --out-dir "$APP_PUBLIC_INSTITUTION_OUT" \
    --chain-id citizenchain

PUBLIC_INSTITUTION_ROOT="$(python3 - "$APP_PUBLIC_INSTITUTION_OUT" "$APP_OUT" "$GENESIS_HASH_STR" "$STATE_ROOT" <<'PYEOF'
import hashlib
import json
import os
import sys

bundle_dir, app_spec_path, genesis_hash, state_root = sys.argv[1:]
manifest_path = os.path.join(bundle_dir, "manifest.json")
with open(manifest_path, encoding="utf-8") as f:
    manifest = json.load(f)

def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()

expected_manifest_fields = {
    "chain_id", "snapshot_block_number", "snapshot_block_hash",
    "genesis_hash", "state_root", "chainspec_hash", "public_institution_root",
    "version", "shard_hashes", "provinces",
}
if manifest.get("chain_id") != "citizenchain" or set(manifest) != expected_manifest_fields:
    raise SystemExit("公权机构 manifest 身份无效")
if manifest.get("snapshot_block_number") != 0:
    raise SystemExit("创世公权机构缓存必须钉死块 0")
if str(manifest.get("snapshot_block_hash", "")).lower() != genesis_hash.lower():
    raise SystemExit("公权机构 snapshot_block_hash 与创世哈希不一致")
if str(manifest.get("genesis_hash", "")).lower() != genesis_hash.lower():
    raise SystemExit("公权机构 genesis_hash 与创世哈希不一致")
if str(manifest.get("state_root", "")).lower() != state_root.lower():
    raise SystemExit("块 0 公权机构 state_root 与创世状态根不一致")
if manifest.get("chainspec_hash") != sha256_file(app_spec_path):
    raise SystemExit("公权机构 chainspec_hash 与本次轻形态 chainspec 不一致")

provinces = manifest.get("provinces")
if not isinstance(provinces, list) or len(provinces) != 43:
    raise SystemExit("创世公权机构缓存必须精确包含 43 个省级分片")
for item in provinces:
    province_name = item.get("province_name")
    shard_path = os.path.join(bundle_dir, f"{province_name}.json")
    if not os.path.isfile(shard_path):
        raise SystemExit(f"公权机构分片缺失:{province_name}")
    if item.get("shard_hash") != sha256_file(shard_path):
        raise SystemExit(f"公权机构分片哈希不一致:{province_name}")
root_json = json.dumps(provinces, ensure_ascii=False, separators=(",", ":"))
computed_root = hashlib.sha256(root_json.encode()).hexdigest()
if manifest.get("public_institution_root") != computed_root:
    raise SystemExit("公权机构 public_institution_root 校验失败")
print(computed_root)
PYEOF
)"
echo "==> 公权机构缓存根: $PUBLIC_INSTITUTION_ROOT"

echo "==> 暂存 Cloudflare 公开链身份配置..."
python3 - "$REPO_ROOT/citizenserve/scripts/wrangler.toml" "$CLOUDFLARE_WRANGLER_OUT" "$GENESIS_HASH_STR" "$STATE_ROOT" <<'PYEOF'
import os
import re
import sys

source_path, out_path, genesis_hash, state_root = sys.argv[1:]
with open(source_path, encoding="utf-8") as f:
    text = f.read()
text, genesis_count = re.subn(
    r'^(\s*CHAIN_GENESIS_HASH\s*=\s*)"[^"]*"\s*$',
    lambda match: f'{match.group(1)}"{genesis_hash}"',
    text,
    flags=re.MULTILINE,
)
text, state_count = re.subn(
    r'^(\s*CHAIN_STATE_ROOT\s*=\s*)"[^"]*"\s*$',
    lambda match: f'{match.group(1)}"{state_root}"',
    text,
    flags=re.MULTILINE,
)
if genesis_count == 0 or state_count == 0 or genesis_count != state_count:
    raise SystemExit("Cloudflare wrangler.toml 链身份配置数量异常")
os.makedirs(os.path.dirname(out_path), exist_ok=True)
with open(out_path, "w", encoding="utf-8") as f:
    f.write(text)
print(f"    {out_path} ({genesis_count} 个环境)")
PYEOF

kill "$NODE_PID" 2>/dev/null || true
wait "$NODE_PID" 2>/dev/null || true
NODE_PID=""

echo "==> 生成创世链状态包(供节点安装包首启直接复制链数据库)..."
rm -rf "$GENESIS_STATE_OUT"
mkdir -p "$GENESIS_STATE_OUT/chains/citizenchain"
if [[ ! -d "$NODE_TMP_DIR/chains/citizenchain/db" ]]; then
    echo "错误: 临时节点未生成 chains/citizenchain/db,无法制作创世链状态包。" >&2
    find "$NODE_TMP_DIR" -maxdepth 4 -type d | sort >&2
    exit 1
fi
cp -a "$NODE_TMP_DIR/chains/citizenchain/db" "$GENESIS_STATE_OUT/chains/citizenchain/db"
ARTIFACT_STAGE="preview"
[[ "$FINALIZE" == "1" ]] && ARTIFACT_STAGE="release"
python3 - "$GENESIS_STATE_OUT/manifest.json" "$GENESIS_HASH_STR" "$STATE_ROOT" "$TMP" "${WASM_FILE:-}" "$APP_LIGHT_SYNC_STATE_OUT" "$PUBLIC_INSTITUTION_ROOT" "$GENESIS_SECS" "$WASM_CI_RUN_ID" "$WASM_CI_HEAD_SHA" "$ARTIFACT_STAGE" <<'PYEOF'
import datetime
import hashlib
import json
import os
import sys

manifest_path, genesis_hash, state_root, chainspec_path, wasm_path, light_sync_state_path, public_institution_root, secs, wasm_ci_run_id, wasm_ci_head_sha, artifact_stage = sys.argv[1:]

def sha256_file(path):
    if not path or not os.path.isfile(path):
        return ""
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()

def sha256_runtime_code(path):
    with open(path, encoding="utf-8") as f:
        spec = json.load(f)
    code = spec.get("genesis", {}).get("runtimeGenesis", {}).get("code", "")
    if not isinstance(code, str) or not code.startswith("0x"):
        return ""
    return hashlib.sha256(bytes.fromhex(code[2:])).hexdigest()

manifest = {
    "package_format": "citizenchain-genesis-state",
    "chain_id": "citizenchain",
    "artifact_stage": artifact_stage,
    "snapshot_block_number": 0,
    "snapshot_block_hash": genesis_hash,
    "genesis_hash": genesis_hash,
    "state_root": state_root,
    "chainspec_hash": sha256_file(chainspec_path),
    "runtime_wasm_hash": sha256_file(wasm_path) or sha256_runtime_code(chainspec_path),
    "runtime_wasm_ci_run_id": wasm_ci_run_id,
    "runtime_wasm_ci_head_sha": wasm_ci_head_sha,
    "light_sync_state_hash": sha256_file(light_sync_state_path),
    "public_institution_root": public_institution_root,
    "genesis_materialization_secs": int(secs),
    "included_paths": ["chains/citizenchain/db"],
    "generated_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
}
with open(manifest_path, "w", encoding="utf-8") as f:
    json.dump(manifest, f, ensure_ascii=False, indent=2)
    f.write("\n")
print(f"    {manifest_path}")
PYEOF

validate_genesis_state_package "$GENESIS_STATE_OUT" "$FINALIZE"
echo "==> 创世链状态包白名单校验通过:仅包含 manifest.json 与 chains/citizenchain/db"

mv "$TMP" "$OUT"
trap - EXIT
rm -rf "$NODE_TMP_DIR"
echo "==> 已生成: $OUT"
echo "==> 首启物化耗时 ${GENESIS_SECS}s(验收记录);创世哈希 $GENESIS_HASH_STR"

if [[ "$FINALIZE" == "1" ]]; then
    echo "==> 正式覆盖前校验全部暂存发布物..."
    CITIZENAPP_CHAINSPEC="$APP_OUT" \
    CITIZENAPP_LIGHT_SYNC_STATE="$APP_LIGHT_SYNC_STATE_OUT" \
    CITIZENCHAIN_PLAIN_SPEC="$OUT" \
    CITIZENCHAIN_GENESIS_STATE_MANIFEST="$GENESIS_STATE_OUT/manifest.json" \
    CITIZENAPP_PUBLIC_INSTITUTION_MANIFEST="$APP_PUBLIC_INSTITUTION_OUT/manifest.json" \
    CITIZENSERVE_CLOUDFLARE_WRANGLER="$CLOUDFLARE_WRANGLER_OUT" \
    CITIZENAPP_REQUIRE_STATE_ROOT=1 \
        "$REPO_ROOT/citizenapp/scripts/check-chainspec-frozen.sh"

    NODE_SPEC="$CHAIN_ROOT/node/citizenchain.json"
    APP_SPEC="$REPO_ROOT/citizenapp/assets/chainspec.json"
    APP_LIGHT_SYNC_STATE="$REPO_ROOT/citizenapp/assets/light_sync_state.json"
    APP_PUBLIC_INSTITUTION="$REPO_ROOT/citizenapp/assets/public_institutions"
    CLOUDFLARE_WRANGLER="$REPO_ROOT/citizenserve/scripts/wrangler.toml"
    install -m 0644 "$OUT" "$NODE_SPEC"
    install -m 0644 "$APP_OUT" "$APP_SPEC"
    install -m 0644 "$APP_LIGHT_SYNC_STATE_OUT" "$APP_LIGHT_SYNC_STATE"
    rm -rf "$APP_PUBLIC_INSTITUTION"
    mkdir -p "$APP_PUBLIC_INSTITUTION"
    cp -a "$APP_PUBLIC_INSTITUTION_OUT/." "$APP_PUBLIC_INSTITUTION/"
    install -m 0644 "$CLOUDFLARE_WRANGLER_OUT" "$CLOUDFLARE_WRANGLER"
    echo "==> 已同步冻结 SSOT:"
    echo "    $NODE_SPEC"
    echo "    $APP_SPEC (轻形态 stateRootHash)"
    echo "    $APP_LIGHT_SYNC_STATE (lightSyncState checkpoint)"
    echo "    $APP_PUBLIC_INSTITUTION (块 0 公权机构缓存)"
    echo "    $CLOUDFLARE_WRANGLER (公开链身份派生配置)"
    echo "==> 正式创世审计状态包已生成（安装包仍按冻结 plain chainspec 本地物化）:"
    echo "    $GENESIS_STATE_OUT"
else
    echo "==> 预览模式完成,未覆盖冻结 SSOT。正式创世请加 --finalize --wasm <CI_WASM>。"
fi

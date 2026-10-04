#!/usr/bin/env bash
# 下载指定成功运行的 CitizenChain WASM CI artifact 到 citizenchain/target/wasm-ci/。
# 正式创世必须同时钉死 run id、提交 SHA 和候选 tag，禁止按“最新成功”推断产物来源。

set -euo pipefail

# 补充常见的 PATH，确保 gh CLI 可用
export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"

REPO="crcfrcn/citizenchain"
WORKFLOW="CitizenChain WASM"
ARTIFACT_NAME="citizenchain-wasm"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CITIZENCHAIN_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$CITIZENCHAIN_DIR/target/wasm-ci"
RUN_ID=""
EXPECTED_HEAD_SHA=""
EXPECTED_REF=""

usage() {
  cat <<'EOF'
用法:
  citizenchain/scripts/download-wasm.sh \
    --run-id <RUN_ID> \
    --head-sha <40位小写提交SHA> \
    --ref <候选tag>

说明:
  只下载指定 CitizenChain WASM workflow_dispatch 成功运行的 artifact。
  run id、提交 SHA、候选 tag 任一不匹配都会失败关闭。
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --run-id)
      RUN_ID="${2:?--run-id 需要 GitHub Actions run id}"
      shift 2
      ;;
    --head-sha)
      EXPECTED_HEAD_SHA="${2:?--head-sha 需要 40 位小写提交 SHA}"
      shift 2
      ;;
    --ref)
      EXPECTED_REF="${2:?--ref 需要候选 tag}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "错误：未知参数 $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ ! "$RUN_ID" =~ ^[0-9]+$ ]]; then
  echo "错误：--run-id 必须为纯数字。" >&2
  exit 2
fi
if [[ ! "$EXPECTED_HEAD_SHA" =~ ^[0-9a-f]{40}$ ]]; then
  echo "错误：--head-sha 必须为 40 位小写十六进制提交 SHA。" >&2
  exit 2
fi
if [[ "$EXPECTED_REF" == "main" || ! "$EXPECTED_REF" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]]; then
  echo "错误：--ref 必须是候选 tag 短名称，只允许字母、数字、点、下划线和连字符。" >&2
  exit 2
fi

if ! command -v gh >/dev/null 2>&1; then
  echo "错误：未找到 gh CLI，请先安装并执行 gh auth login" >&2
  exit 1
fi

if ! TAG_TARGET_SHA=$(gh api "repos/$REPO/git/ref/tags/$EXPECTED_REF" \
  --jq 'if .object.type == "commit" then .object.sha else empty end' 2>/dev/null); then
  echo "错误：远端候选 tag ${EXPECTED_REF} 不存在或无法读取。" >&2
  exit 1
fi
if [[ "$TAG_TARGET_SHA" != "$EXPECTED_HEAD_SHA" ]]; then
  echo "错误：候选 tag ${EXPECTED_REF} 不是直接指向预期提交的轻量 tag。" >&2
  exit 1
fi

echo "核验指定的 $WORKFLOW 运行：$RUN_ID"
IFS=$'\t' read -r ACTUAL_WORKFLOW STATUS CONCLUSION ACTUAL_HEAD_SHA ACTUAL_REF EVENT RUN_URL < <(
  gh run view "$RUN_ID" \
    --repo "$REPO" \
    --json workflowName,status,conclusion,headSha,headBranch,event,url \
    --jq '[.workflowName, .status, .conclusion, .headSha, .headBranch, .event, .url] | @tsv'
)

if [[ "$ACTUAL_WORKFLOW" != "$WORKFLOW" ]]; then
  echo "错误：run $RUN_ID 属于 ${ACTUAL_WORKFLOW}，不是 ${WORKFLOW}。" >&2
  exit 1
fi
if [[ "$STATUS" != "completed" || "$CONCLUSION" != "success" ]]; then
  echo "错误：run $RUN_ID 尚未成功完成：status=$STATUS conclusion=${CONCLUSION}。" >&2
  exit 1
fi
if [[ "$EVENT" != "workflow_dispatch" ]]; then
  echo "错误：run $RUN_ID 不是 workflow_dispatch：event=${EVENT}。" >&2
  exit 1
fi
if [[ "$ACTUAL_HEAD_SHA" != "$EXPECTED_HEAD_SHA" ]]; then
  echo "错误：run $RUN_ID 的提交 SHA 不匹配：${ACTUAL_HEAD_SHA}。" >&2
  exit 1
fi
if [[ "$ACTUAL_REF" != "$EXPECTED_REF" ]]; then
  echo "错误：run $RUN_ID 的 ref 不匹配：${ACTUAL_REF}。" >&2
  exit 1
fi

ARTIFACT_ID=$(gh api "repos/$REPO/actions/runs/$RUN_ID/artifacts" \
  --jq '[.artifacts[] | select(.name == "'"$ARTIFACT_NAME"'" and .expired == false)] |
        if length == 1 then .[0].id else empty end')
if [[ ! "$ARTIFACT_ID" =~ ^[0-9]+$ ]]; then
  echo "错误：run $RUN_ID 必须且只能有一个未过期的 $ARTIFACT_NAME artifact。" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/gmb-wasm-ci.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "下载 artifact $ARTIFACT_ID 到临时校验目录..."
gh run download "$RUN_ID" \
  --repo "$REPO" \
  --name "$ARTIFACT_NAME" \
  --dir "$TMP_DIR"

EXPECTED_FILES=(
  "citizenchain.wasm"
  "citizenchain.compact.wasm"
  "citizenchain.compact.compressed.wasm"
)
for filename in "${EXPECTED_FILES[@]}"; do
  if [[ ! -s "$TMP_DIR/$filename" ]]; then
    echo "错误：artifact 缺少非空文件 ${filename}。" >&2
    exit 1
  fi
done
ACTUAL_WASM_COUNT=$(find "$TMP_DIR" -maxdepth 1 -type f -name "*.wasm" | wc -l | tr -d ' ')
if [[ "$ACTUAL_WASM_COUNT" != "${#EXPECTED_FILES[@]}" ]]; then
  echo "错误：artifact 中 WASM 文件数量异常：${ACTUAL_WASM_COUNT}。" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
find "$OUT_DIR" -maxdepth 1 -type f -name "*.wasm" -delete
for filename in "${EXPECTED_FILES[@]}"; do
  cp "$TMP_DIR/$filename" "$OUT_DIR/$filename"
done

echo
echo "完成："
echo "run_id=$RUN_ID"
echo "artifact_id=$ARTIFACT_ID"
echo "head_sha=$ACTUAL_HEAD_SHA"
echo "ref=$ACTUAL_REF"
echo "run_url=$RUN_URL"
python3 - "$OUT_DIR" "${EXPECTED_FILES[@]}" <<'PY'
import hashlib
import sys
from pathlib import Path

output_dir = Path(sys.argv[1])
for filename in sys.argv[2:]:
    data = (output_dir / filename).read_bytes()
    digest = hashlib.blake2b(data, digest_size=32).hexdigest()
    print(f"{filename:50s} size={len(data):>10d}  blake2_256=0x{digest}")
PY

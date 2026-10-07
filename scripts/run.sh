#!/usr/bin/env bash
# CitizenChain节点本机Build入口：只构建、签名验真并写入产品产物目录。
# 本入口不启动或停止节点，不修改链数据；启动由产品独立Start入口负责。
# 测试、手工调试和清库不走本入口。
set -euo pipefail

MACOS_APP_BUNDLE=''
MACOS_APP_PENDING=0

cleanup() {
    local result="${1:-1}"
    trap - EXIT INT TERM HUP
    # 唯一成功路径会撤销 trap；其余提前退出必须失败，避免 Bash 参数展开错误被清理返回值吞掉。
    [[ "$result" != 0 ]] || result=1
    # 只清理本轮尚未通过完整签名验收的固定 App；历史成功归档和用户数据均不触碰。
    if [[ "${MACOS_APP_PENDING:-0}" == 1 \
        && -n "${TARGET_DIR:-}" \
        && "${MACOS_APP_BUNDLE:-}" == "$TARGET_DIR/release/bundle/macos/citizenchain.app" ]]; then
        rm -rf -- "$MACOS_APP_BUNDLE"
    fi
    # Build 不启动节点或 PG；校验失败不得停止其它任务或用户已运行的实例。
    exit "$result"
}
trap 'cleanup "$?"' EXIT
trap 'cleanup 130' INT
trap 'cleanup 143' TERM
trap 'cleanup 129' HUP

# macOS 产品 App 只接受今后唯一的新团队 Developer ID；禁止按枚举顺序选证书，
# 否则Apple Development或其它团队身份可能被静默当成产品正式签名。
MACOS_SIGNING_IDENTITY='Developer ID Application: WEI CHENG (MHYMVRN6FC)'
MACOS_TEAM_ID='MHYMVRN6FC'
MACOS_BUNDLE_ID='macOS.citizenappchain'
MACOS_APPLICATION_ID="${MACOS_TEAM_ID}.${MACOS_BUNDLE_ID}"
MACOS_PROFILE_NAME='CitizenChain Developer ID'
MACOS_PROFILE_DIR="$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles"

require_macos_signing_identity() {
    /usr/bin/security find-identity -v -p codesigning 2>/dev/null \
        | grep -F "\"$MACOS_SIGNING_IDENTITY\"" >/dev/null
}

# Apple 安全时间戳服务偶发不可用时只重试签名相关步骤。错误不属于该服务、
# 或三次重试仍失败时立即关闭，不允许改用无时间戳、临时签名等降级路径。
MACOS_TIMESTAMP_RETRY_DELAYS=(2 5 10)

is_macos_timestamp_service_failure() {
    local output="$1"
    [[ "$output" == *"A timestamp was expected but was not found"* \
        || "$output" == *"The timestamp service is not available"* \
        || "$output" == *"timestamp service is not available"* ]]
}

run_with_macos_timestamp_retry() {
    local action="$1"
    shift
    local output='' status=0 retry=0 delay
    while true; do
        set +e
        output="$("$@" 2>&1)"
        status=$?
        set -e
        [[ -z "$output" ]] || printf '%s\n' "$output"
        [[ "$status" == 0 ]] && return 0
        is_macos_timestamp_service_failure "$output" || return "$status"
        if (( retry >= ${#MACOS_TIMESTAMP_RETRY_DELAYS[@]} )); then
            echo "    [error] $action 因 Apple 安全时间戳服务异常连续重试三次后仍失败" >&2
            return "$status"
        fi
        delay="${MACOS_TIMESTAMP_RETRY_DELAYS[$retry]}"
        retry=$((retry + 1))
        echo "    [warn] $action 未取得 Apple 安全时间戳，${delay} 秒后执行第 $retry 次重试" >&2
        sleep "$delay"
    done
}

# Apple 重新签发描述文件会改变 UUID，因此禁止绑定文件名。只允许唯一一个同时匹配
# 产品名、Team ID 和公民链 App ID 的 Developer ID 描述文件，重复或旧标识均失败关闭。
resolve_macos_profile() {
    local candidate profile_name application_id team_id matched='' matched_count=0
    shopt -s nullglob
    for candidate in "$MACOS_PROFILE_DIR"/*.provisionprofile "$MACOS_PROFILE_DIR"/*.mobileprovision; do
        profile_name="$(security cms -D -i "$candidate" 2>/dev/null \
            | plutil -extract Name raw -o - - 2>/dev/null || true)"
        application_id="$(security cms -D -i "$candidate" 2>/dev/null \
            | plutil -extract 'Entitlements.com\.apple\.application-identifier' raw -o - - 2>/dev/null || true)"
        team_id="$(security cms -D -i "$candidate" 2>/dev/null \
            | plutil -extract 'Entitlements.com\.apple\.developer\.team-identifier' raw -o - - 2>/dev/null || true)"
        if [[ "$profile_name" == "$MACOS_PROFILE_NAME" \
            && "$application_id" == "$MACOS_APPLICATION_ID" \
            && "$team_id" == "$MACOS_TEAM_ID" ]]; then
            matched="$candidate"
            matched_count=$((matched_count + 1))
        fi
    done
    shopt -u nullglob
    [[ "$matched_count" == 1 ]] || {
        echo "    [error] 必须且只能安装一个匹配 $MACOS_BUNDLE_ID 的 $MACOS_PROFILE_NAME" >&2
        return 1
    }
    printf '%s\n' "$matched"
}

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"   # citizenchain/
CITIZENCHAIN_ROOT="$REPO_ROOT"
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
CITIZENCHAIN_WORK_DIR="${CITIZENCHAIN_WORK_DIR:-${TMPDIR:-$PRODUCT_TARGET_TEMP_ROOT}/citizenchain/macos}"
BUILD_WORK_DIR="${CITIZENCHAIN_BUILD_WORK_DIR:-$CITIZENCHAIN_WORK_DIR/work}"
CITIZENCHAIN_DEPENDENCY_DIR="${CITIZENCHAIN_DEPENDENCY_DIR:-$CITIZENCHAIN_WORK_DIR/dependencies}"
TARGET_DIR="$BUILD_WORK_DIR/cargo-target"
export CARGO_TARGET_DIR="$TARGET_DIR"
NODE_FRONTEND_DIST="$CITIZENCHAIN_WORK_DIR/node-frontend"
ONCHINA_BUILD_DIST="$CITIZENCHAIN_WORK_DIR/onchina-frontend/dist"
PACKAGE_RESOURCES="$CITIZENCHAIN_WORK_DIR/resources"
# Build只形成当前调用的候选App；正式Release与Publish由产品发布流程另行验真。
ARTIFACT_DIR="${CITIZENCHAIN_ARTIFACT_DIR:-$CITIZENCHAIN_WORK_DIR/artifacts}"

# 产品自行使用当前环境中的工具，并按自己的锁文件在源码外工作目录准备依赖。
source "$CITIZENCHAIN_ROOT/scripts/prepare-toolchain.sh"

# 本机Build脚本只使用当前工作区源码构建 runtime WASM，禁止接受外部 WASM 覆盖。
unset WASM_FILE
# Cargo/Tauri 的 release profile 只是本机优化配置；gmb.dev 是本机开发数据隔离环境。
# 本任务不修改正式gmb数据，也不与正式安装版争用RocksDB。
export CITIZENCHAIN_DATA_PROFILE=dev
mkdir -p "$TARGET_DIR" "$npm_config_cache" "$PACKAGE_RESOURCES/onchina-bin" "$PACKAGE_RESOURCES/onchina-frontend"

# ── OnChina 控制台本机配置 ──
# 启动节点不需要任何机构鉴权/身份。这里只让本机能跑起链上中国平台服务:
#   ① 构建 onchina 二进制(节点同目录,设置页手动启动时由 onchina_proc 拉起)+ 前端产物;
#   ② DB 用内嵌私有 PG(方案 A):借本机 PostgreSQL 二进制起一个 onchina 专属实例(127.0.0.1)。
# 本机构的"系统签名钥 / 机构身份"是可选配置(签登录 QR / 签发凭证才需要),非启动前提。
echo "==> 构建 OnChina 本机优化二进制 + 前端..."
( cd "$REPO_ROOT" && CARGO_INCREMENTAL=1 cargo build --locked --release -p onchina --config "$REPO_ROOT/config.toml" )
echo "==> 构建链上中国平台前端产物..."
( cd "$ONCHINA_FRONTEND_PROJECT" && ONCHINA_FRONTEND_DIST="$ONCHINA_BUILD_DIST" npm run build )
echo "==> 构建节点前端产物..."
( cd "$NODE_FRONTEND_PROJECT" && CITIZENCHAIN_FRONTEND_DIST="$NODE_FRONTEND_DIST" npm run build )
cp "$TARGET_DIR/release/onchina" "$PACKAGE_RESOURCES/onchina-bin/onchina"
cp -R "$ONCHINA_BUILD_DIST" "$PACKAGE_RESOURCES/onchina-frontend/dist"
PG_PREFIX=""
for v in postgresql@17 postgresql@16 postgresql@15 postgresql; do
    if p="$(brew --prefix "$v" 2>/dev/null)" && [ -x "$p/bin/initdb" ]; then PG_PREFIX="$p"; break; fi
done
if [ -n "$PG_PREFIX" ]; then
    export ONCHINA_EMBEDDED_PG=1
    export ONCHINA_PG_BIN_DIR="$PG_PREFIX/bin"
    export ONCHINA_PG_PORT="${ONCHINA_PG_PORT:-5433}"
    export ONCHINA_PG_DATA_DIR="$HOME/Library/Application Support/gmb.dev/onchina-pgdata"
    echo "    内嵌私有 PG:$ONCHINA_PG_BIN_DIR(端口 $ONCHINA_PG_PORT)"
else
    echo "    [warn] 未找到本机 PostgreSQL(brew install postgresql@16);链上中国平台仍可起但缺 DB,功能受限。"
fi
export ONCHINA_CHINA_DB="$REPO_ROOT/onchina/src/cid/china/china.sqlite"
export ONCHINA_FRONTEND_DIST="$ONCHINA_BUILD_DIST"
export ONCHINA_ENABLE_TLS=1
export ONCHINA_TLS_DIR="$HOME/Library/Application Support/gmb.dev/onchina-tls"
# 公权机构目录只允许从链上投影到本地缓存;开发启动不再打开旧本地生成开关。
# 链不可达或投影不可读时,链上中国按 fail-closed 不放行平台服务。
# OnChina 后端不再持有任何链上签名钥:机构操作全部由管理员冷钱包直接冷签,
# 原平台签名钥与注销凭证签发配置已随注销凭证链路整体删除。

echo "==> 使用当前工作区源码构建本机 runtime WASM..."
echo "    节点Build产物目录: $TARGET_DIR"
echo "    本机运行数据目录: $HOME/Library/Application Support/gmb.dev"
echo "==> 链上中国平台:节点设置页点击「启动」后访问 https://onchina.local:8964"

# ── 构建与签名封装 ──
cd "$REPO_ROOT/node"
echo "==> 构建公民链..."
if [[ "$(uname -s)" == "Darwin" ]]; then
    require_macos_signing_identity || {
        echo "    [error] 未找到唯一允许的新团队 Developer ID：$MACOS_SIGNING_IDENTITY" >&2
        exit 1
    }
    export APPLE_SIGNING_IDENTITY="$MACOS_SIGNING_IDENTITY"
    macos_profile="$(resolve_macos_profile)"
    echo "    使用新团队 Developer ID 构建带摄像头权限的本机优化 App"
    # 使用当前任务按原始 lockfile 安装的 CLI，不能读取源码 node_modules 或全局 CLI。
    app_bundle="$TARGET_DIR/release/bundle/macos/citizenchain.app"
    MACOS_APP_BUNDLE="$app_bundle"
    # Tauri 2 的 build 默认使用优化 profile；--debug 才会切换为调试产物。
    # 编译与封装分离，时间戳瞬时失败时只重试封装签名，不重复整轮 Rust 编译。
    # 前端已在私有工程完成构建；清除 Tauri 的源码 npm 钩子，避免重复构建或回写主仓。
    tauri_override="$(python3 -c 'import json,sys; print(json.dumps({"build":{"beforeBuildCommand":None,"frontendDist":sys.argv[1]},"bundle":{"resources":{sys.argv[2]+"/":"",sys.argv[3]+"/":"",sys.argv[4]:"china.sqlite"}}}))' "$NODE_FRONTEND_DIST" "$PACKAGE_RESOURCES" "$REPO_ROOT/node/resources" "$REPO_ROOT/onchina/src/cid/china/china.sqlite")"
    CITIZENCHAIN_FRONTEND_DIST="$NODE_FRONTEND_DIST" CARGO_INCREMENTAL=1 \
        node "$NODE_FRONTEND_PROJECT/node_modules/@tauri-apps/cli/tauri.js" build --config "$tauri_override" \
        --no-bundle --ci -- --locked --config "$REPO_ROOT/config.toml"
    MACOS_APP_PENDING=1
    bundle_macos_app() {
        rm -rf -- "$app_bundle"
        CITIZENCHAIN_FRONTEND_DIST="$NODE_FRONTEND_DIST" CARGO_INCREMENTAL=1 \
            node "$NODE_FRONTEND_PROJECT/node_modules/@tauri-apps/cli/tauri.js" bundle --config "$tauri_override" \
            --bundles app --ci
    }
    run_with_macos_timestamp_retry "Tauri App 封装签名" bundle_macos_app

    app_plist="$app_bundle/Contents/Info.plist"
    app_executable="$app_bundle/Contents/MacOS/citizenchain"
    [[ -x "$app_executable" ]] || {
        echo "    [error] Tauri 构建完成但缺少 App 主程序：$app_executable" >&2
        exit 1
    }
    # Tauri 负责生成产品 App；描述文件属于本机签名材料，绝不进入仓库或其它产物路径。
    # 嵌入后必须重新封签根 App，使描述文件、唯一 App ID 与权限成为同一个签名整体。
    /usr/bin/ditto "$macos_profile" "$app_bundle/Contents/embedded.provisionprofile"
    run_with_macos_timestamp_retry "描述文件嵌入后的 App 封签" \
        /usr/bin/codesign --force --deep --options runtime --timestamp \
        --entitlements "$REPO_ROOT/node/Entitlements.plist" \
        --sign "$MACOS_SIGNING_IDENTITY" "$app_bundle"
    codesign --verify --deep --strict "$app_bundle"
    signature_details="$(codesign -dv --verbose=4 "$app_bundle" 2>&1)"
    grep -Fqx "Authority=$MACOS_SIGNING_IDENTITY" <<<"$signature_details" || {
        echo "    [error] macOS App 未使用唯一允许的新团队 Developer ID" >&2
        exit 1
    }
    grep -Fqx "TeamIdentifier=$MACOS_TEAM_ID" <<<"$signature_details" || {
        echo "    [error] macOS App Team ID 不属于唯一允许的新团队" >&2
        exit 1
    }
    grep -Eq '^CodeDirectory .*flags=.*\(runtime\)' <<<"$signature_details" || {
        echo "    [error] macOS App 未启用 Hardened Runtime" >&2
        exit 1
    }
    grep -Eq '^Timestamp=' <<<"$signature_details" || {
        echo "    [error] macOS App 缺少安全时间戳" >&2
        exit 1
    }
    [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$app_plist")" == "$MACOS_BUNDLE_ID" ]] || {
        echo "    [error] macOS App Bundle ID 与 Tauri 配置不一致" >&2
        exit 1
    }
    embedded_profile_name="$(security cms -D -i "$app_bundle/Contents/embedded.provisionprofile" 2>/dev/null \
        | plutil -extract Name raw -o - - 2>/dev/null || true)"
    [[ "$embedded_profile_name" == "$MACOS_PROFILE_NAME" ]] || {
        echo "    [error] macOS App 未嵌入 $MACOS_PROFILE_NAME" >&2
        exit 1
    }
    /usr/libexec/PlistBuddy -c 'Print :NSCameraUsageDescription' "$app_plist" >/dev/null
    signed_entitlements="$(codesign -d --entitlements :- "$app_bundle" 2>/dev/null)"
    for entitlement in \
        com.apple.security.device.camera \
        com.apple.security.cs.allow-jit \
        com.apple.security.cs.allow-unsigned-executable-memory; do
        # plutil 把点号解释为字典层级；entitlement 名本身含点号，必须先转义为单个键。
        entitlement_key_path="${entitlement//./\\.}"
        [[ "$(printf '%s' "$signed_entitlements" | plutil -extract "$entitlement_key_path" raw -)" == "true" ]] || {
            echo "    [error] macOS App 签名缺少 $entitlement" >&2
            exit 1
        }
    done
    get_task_allow="$(printf '%s' "$signed_entitlements" \
        | plutil -extract 'com\.apple\.security\.get-task-allow' raw - 2>/dev/null \
        || printf 'false')"
    [[ "$get_task_allow" == "false" ]] || {
        echo "    [error] macOS 本机优化 App 禁止 get-task-allow" >&2
        exit 1
    }
    MACOS_APP_PENDING=0
    echo "    本机优化路径、新团队签名、Bundle ID、Hardened Runtime、安全时间戳与 entitlement 校验通过"
    # 成功后只保存本次缓存根的候选 App；不得触碰正式产物库。
    mkdir -p "$ARTIFACT_DIR"
    rm -rf "$ARTIFACT_DIR/CitizenChain.app"
    mv "$app_bundle" "$ARTIFACT_DIR/CitizenChain.app"
    app_bundle="$ARTIFACT_DIR/CitizenChain.app"
    app_executable="$app_bundle/Contents/MacOS/citizenchain"
    export ONCHINA_FRONTEND_DIST="$app_bundle/Contents/Resources/onchina-frontend/dist"
    # Build只生成并验真产品产物，不终止旧实例，也不启动新实例。
    trap - EXIT INT TERM HUP
    echo "    CitizenChain Node macOS候选产物构建完成；Build不会启动节点"
else
    echo "    [error] 本入口只负责macOS Build；其它平台使用各自产品构建入口" >&2
    exit 1
fi

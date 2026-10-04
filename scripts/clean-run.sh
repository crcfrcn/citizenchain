#!/usr/bin/env bash
# 清库后二选一启动(杀进程 + 删本机链数据 + 删链上中国运行时 PG,再按模式启动):
#   [1] 冻结 SSOT + 从网络同步:用【冻结 SSOT】(node/citizenchain.json)
#       启动,作为新节点从区块链网络同步区块。
#       要求:冻结创世 = 现网创世、且有可达 bootnode;本机为唯一节点时同步不到对等数据。
#   [2] 隔离 fresh 新链:用【当前源码 genesis_build】启动一条独立本地新链。
#       改了 block#0 配置(宪法/立法院/账户…)无需重烤 SSOT 即时生效 —— 本地验证 fresh 入口。
#
# 两模式都删:chains/citizenchain/db(区块+状态)+ onchina-pgdata(链上中国运行时 PG,可从链重投影)。
# 两模式都保留:node-key(PeerId)/keystore(矿工密钥)/tls(WSS 证书)= 与创世无关的节点身份。
# 两模式都不动:china.sqlite(行政区只读源数据)。
#
# 与 run.sh 的区别:run.sh 不删任何数据、用冻结 SSOT 续跑现有链。
#
# 机制:节点启动读 CITIZENCHAIN_CHAIN_SPEC;设为 citizenchain-fresh 即走
#   chain_spec::fresh_genesis_config()(当前源码 fresh),不读冻结 JSON;不设则用冻结 SSOT。
#   fresh genesis 需 runtime WASM,故模式 2 用 WASM_BUILD_FROM_SOURCE=1 从源码编 WASM。
#
# 启动后:节点自动挖矿;链上中国平台需在节点设置页手动启动,统一入口 https://onchina.local:8964。
#   平台登录与节点启动解耦,本机构管理员冷钱包扫码、对链上 Active 管理员集合鉴权(3b)即可登录。
#
# 代价(模式 2):fresh genesis :code = 本地 WASM,与现网不逐字节一致 → 独立本地链;
#   要做全网共识需用同一份 CI WASM 经 bake-chainspec.sh --finalize 重生冻结 plain spec 分发;
#   首次需从源码编译 runtime WASM,较慢。
set -euo pipefail

# ── 0. 选择启动模式(可传参 ./clean-run.sh 1|2 免交互)──
MODE="${1:-}"
if [ -z "$MODE" ]; then
    echo "请选择启动模式:"
    echo "  [1] 冻结 SSOT + 从网络同步(删库,用冻结 SSOT 启动)"
    echo "  [2] 隔离 fresh 新链  (删库,用当前源码 genesis_build 启动)"
    read -rp "输入 1 或 2: " MODE
fi
case "$MODE" in
    1) echo "==> 模式 1:冻结 SSOT + 从网络同步" ;;
    2) echo "==> 模式 2:隔离 fresh 新链" ;;
    *) echo "[error] 无效选择:'$MODE'(只能是 1 或 2)" >&2; exit 1 ;;
esac

APP_DATA_DIR="$HOME/Library/Application Support/gmb.dev"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHAIN_ROOT="$(dirname "$SCRIPT_DIR")"   # citizenchain/
CITIZENCHAIN_ROOT="$CHAIN_ROOT"

CITIZENCHAIN_WORK_DIR="${CITIZENCHAIN_WORK_DIR:-${TMPDIR:-/tmp}/citizenchain/clean-run}"
CITIZENCHAIN_DEPENDENCY_DIR="${CITIZENCHAIN_DEPENDENCY_DIR:-$CITIZENCHAIN_WORK_DIR/dependencies}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$CITIZENCHAIN_WORK_DIR/cargo-target}"
# 在清库前先按产品锁文件准备工具依赖；生成状态只进入本次源码外工作目录。
source "$CITIZENCHAIN_ROOT/scripts/prepare-toolchain.sh"
NODE_FRONTEND_DIST="$CITIZENCHAIN_WORK_DIR/node-frontend"
ONCHINA_BUILD_DIST="$CITIZENCHAIN_WORK_DIR/onchina-frontend/dist"

cleanup() {
    echo ""
    echo "==> 正在关闭节点 + 链上中国平台 + 内嵌 PG..."
    pkill -f "citizenchain" 2>/dev/null || true
    pkill -f "target/debug/onchina" 2>/dev/null || true
    lsof -ti:5173 2>/dev/null | xargs kill -9 2>/dev/null || true
    if [ -n "${ONCHINA_PG_BIN_DIR:-}" ] && [ -n "${ONCHINA_PG_DATA_DIR:-}" ] && [ -d "${ONCHINA_PG_DATA_DIR:-}" ]; then
        "$ONCHINA_PG_BIN_DIR/pg_ctl" stop -D "$ONCHINA_PG_DATA_DIR" -m fast >/dev/null 2>&1 || true
    fi
    sleep 1
    echo "    已关闭"
}
trap cleanup EXIT INT TERM HUP

# ── 1. 杀进程 ──
echo "==> 杀掉所有节点/链上中国平台进程..."
pkill -9 -f "citizenchain" 2>/dev/null || true
pkill -9 -f "target/debug/onchina" 2>/dev/null || true
lsof -ti:5173 2>/dev/null | xargs kill -9 2>/dev/null || true
sleep 1
echo "    已清理"

# ── 2. 完全删除区块链数据(清链)+ 链上中国平台内嵌 PG 数据(与新创世一致地全新)──
# 只删 db/(区块 + 状态)= 完全清链;node-key(PeerId)、keystore(矿工密钥)、
#   tls/(WSS 证书)是与创世无关的节点身份,保留以免重新生成。
DB_DIR="$APP_DATA_DIR/chains/citizenchain/db"
ONCHINA_PGDATA="$APP_DATA_DIR/onchina-pgdata"
echo "==> 完全删除区块链数据:$DB_DIR"
rm -rf "$DB_DIR"
echo "==> 删除链上中国平台运行时 PG 数据(重新 initdb,数据后续从链重投影):$ONCHINA_PGDATA"
rm -rf "$ONCHINA_PGDATA"
echo "    已清库(node-key/keystore/tls 节点身份保留;china.sqlite 源数据不动)"

# ── 3. onchina 控制台 dev 配置 ──
# 启动节点不需要任何机构鉴权/身份;此处仅准备链上中国平台手动启动所需资源(内嵌 PG + 前端 + china.sqlite)。
echo "==> 构建 onchina 二进制 + 前端..."
( cd "$CHAIN_ROOT" && cargo build -p onchina --config "$CHAIN_ROOT/config.toml" )
echo "==> 构建链上中国平台前端产物..."
( cd "$ONCHINA_FRONTEND_PROJECT" && ONCHINA_FRONTEND_DIST="$ONCHINA_BUILD_DIST" npm run build )
PG_PREFIX=""
for v in postgresql@17 postgresql@16 postgresql@15 postgresql; do
    if p="$(brew --prefix "$v" 2>/dev/null)" && [ -x "$p/bin/initdb" ]; then PG_PREFIX="$p"; break; fi
done
if [ -n "$PG_PREFIX" ]; then
    export ONCHINA_EMBEDDED_PG=1
    export ONCHINA_PG_BIN_DIR="$PG_PREFIX/bin"
    export ONCHINA_PG_PORT="${ONCHINA_PG_PORT:-5433}"
    export ONCHINA_PG_DATA_DIR="$ONCHINA_PGDATA"
    echo "    内嵌私有 PG:$ONCHINA_PG_BIN_DIR(端口 $ONCHINA_PG_PORT)"
else
    echo "    [warn] 未找到本机 PostgreSQL(brew install postgresql@16);链上中国平台仍可起但缺 DB,功能受限。"
fi
export ONCHINA_CHINA_DB="$CHAIN_ROOT/onchina/src/cid/china/china.sqlite"
export ONCHINA_FRONTEND_DIST="$ONCHINA_BUILD_DIST"
export ONCHINA_ENABLE_TLS=1
export ONCHINA_TLS_DIR="$APP_DATA_DIR/onchina-tls"
# 公权机构目录只允许从链上投影到本地缓存;clean-run 不再打开旧本地生成开关。
# 链不可达或投影不可读时,链上中国按 fail-closed 不放行平台服务。
# OnChina 后端不再持有任何链上签名钥:机构操作全部由管理员冷钱包直接冷签,
# 原平台签名钥与注销凭证签发配置已随注销凭证链路整体删除。
echo "==> 联邦注册局管理员省映射:全走链读,不再执行本地 seed"
echo "==> 链上中国平台:节点设置页点击「启动」后访问 https://onchina.local:8964"

# ── 4. 按模式设置启动 env 并启动 ──
unset WASM_FILE
export CITIZENCHAIN_DATA_PROFILE=dev
if [ "$MODE" = "2" ]; then
    # 模式 2:当前源码 fresh genesis(不走冻结 SSOT)。
    export WASM_BUILD_FROM_SOURCE=1                   # build.rs 从源码编 runtime WASM → fresh genesis 可用
    export CITIZENCHAIN_CHAIN_SPEC=citizenchain-fresh # 节点改用 fresh_genesis_config()(当前 genesis_build)
    echo "==> 用当前源码 fresh genesis 启动(genesis_build 现跑,宪法/立法院等 block#0 改动即时生效)..."
else
    # 模式 1:不设 CITIZENCHAIN_CHAIN_SPEC → 默认冻结 SSOT;从网络同步区块。
    echo "==> 用冻结创世(node/citizenchain.json)启动,从网络同步区块..."
    echo "    注意:需冻结创世 = 现网创世、且有可达 bootnode;本机为唯一节点时同步不到对等数据。"
fi
cd "$CHAIN_ROOT/node"
# 开发钩子也显式在任务内的前端运行，不能把 npm 状态写回原始仓库。
tauri_override="$(node -e 'process.stdout.write(JSON.stringify({build:{beforeDevCommand:{script:"npm run dev",cwd:process.argv[1]},frontendDist:process.argv[2]}}))' "$NODE_FRONTEND_PROJECT" "$NODE_FRONTEND_DIST")"
node "$NODE_FRONTEND_PROJECT/node_modules/@tauri-apps/cli/tauri.js" dev --config "$tauri_override" -- --locked

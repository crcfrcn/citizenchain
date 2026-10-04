//! Chain spec 加载入口。
//!
//! 默认入口加载冻结主网 chainspec(plain 形态:WASM + genesis patch + bootnodes)。
//! 正式安装包同时内置已物化的创世链状态包,首启优先复制本地链数据库;缺包时
//! 才由节点经 runtime `GenesisBuilder` 本地物化,作为开发/排障兜底。
//! 当前创世只直铸国家/省/市公权机构;镇级和新增机构运行期注册上链。
//! `citizenchain-fresh` 仅供 `bake-chainspec.sh` 用最新 CI WASM 重生冻结 JSON 使用。
//!
//! 冻结语义(ADR-031 D5):冻结的是 plain JSON(runtime WASM + patch + bootnodes),
//! 创世哈希由其唯一决定;派生全确定性,全网首启物化结果一致。

use sc_chain_spec::{ChainType, NoExtension, Properties};
use sc_network::config::MultiaddrWithPeerId;
use std::str::FromStr;

pub type ChainSpec = sc_service::GenericChainSpec<NoExtension>;

// 主网冻结 chainspec(plain)。文件路径相对本文件:
// citizenchain/node/src/core/chain_spec.rs → ../../citizenchain.json
const CHAIN_SPEC_PLAIN: &[u8] = include_bytes!("../../citizenchain.json");

pub fn chain_config() -> Result<ChainSpec, String> {
    ChainSpec::from_json_bytes(CHAIN_SPEC_PLAIN.to_vec())
        .map_err(|e| format!("加载冻结 chainspec 失败: {e}"))
}

/// 使用当前编译进 node 的 `WASM_BINARY` 生成 fresh genesis chain spec。
///
/// 该入口只给 `bake-chainspec.sh` 重生冻结 plain spec 使用。默认启动
/// 仍走 `chain_config()` 的冻结主网 JSON,避免误改线上 genesis。
pub fn fresh_genesis_config() -> Result<ChainSpec, String> {
    let wasm = citizenchain::WASM_BINARY.ok_or_else(|| {
        "fresh genesis 需要 WASM_BINARY；请通过 WASM_FILE 指向最新 CI WASM 后再构建".to_string()
    })?;
    let mut properties = Properties::new();
    properties.insert("ss58Format".into(), serde_json::json!(2027));
    properties.insert("tokenDecimals".into(), serde_json::json!(2));
    properties.insert("tokenSymbol".into(), serde_json::json!("GMB"));

    // 从冻结主网 chainspec 原样复用已部署的 bootnode 地址(数量以该文件为准,不在此写死:
    // 节点逐台上线时 bootNodes 会增删,注释里记数字必然漂),
    // 让所有清链后的节点继续通过同一组 DNS/PeerId 互联组网,避免变成孤岛。
    let frozen: serde_json::Value = serde_json::from_slice(CHAIN_SPEC_PLAIN)
        .map_err(|e| format!("解析冻结 chainspec 失败: {e}"))?;
    let boot_nodes = frozen
        .get("bootNodes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "冻结 chainspec 缺少 bootNodes 数组".to_string())?
        .iter()
        .map(|v| {
            let s = v
                .as_str()
                .ok_or_else(|| "bootNodes 元素非字符串".to_string())?;
            MultiaddrWithPeerId::from_str(s).map_err(|e| format!("解析 bootnode {s} 失败: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // WASM runtime 在 no_std 下 `get_preset` 永远返回 None,
    // 不能走 `with_genesis_config_preset_name`。直接调用 runtime crate 的 std 版
    // `genesis_config()` 在 host 端构出完整 JSON,再 `with_genesis_config_patch` 注入。
    let genesis_patch = citizenchain::genesis::genesis_config();

    Ok(ChainSpec::builder(wasm, None)
        .with_name("CitizenChain")
        .with_id("citizenchain")
        .with_chain_type(ChainType::Live)
        .with_protocol_id("citizenchain")
        .with_properties(properties)
        .with_boot_nodes(boot_nodes)
        .with_genesis_config_patch(genesis_patch)
        .build())
}

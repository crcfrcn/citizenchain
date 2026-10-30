use sc_chain_spec::ChainType;
use sc_chain_spec::NoExtension;
use sc_chain_spec::Properties;
use sc_network::config::MultiaddrWithPeerId;
use std::str::FromStr;

pub type ChainSpec = sc_service::GenericChainSpec<NoExtension>;

// 主网冻结 chainspec(plain)。文件路径相对本文件:
// citizenchain/node/src/chain.rs → ../citizenchain.json
const CHAIN_SPEC_PLAIN: &[u8] = include_bytes!("../citizenchain.json");

pub(crate) fn chain_config() -> Result<ChainSpec, String> {
    crate::chain::ChainSpec::from_json_bytes(CHAIN_SPEC_PLAIN.to_vec())
        .map_err(|e| format!("加载冻结 chainspec 失败: {e}"))
}

/// 使用当前编译进 node 的 `WASM_BINARY` 生成 fresh genesis chain spec。
///
/// 该入口只给明确授权的冻结链规生成流程使用。默认启动
/// 仍走 `chain_config()` 的冻结主网 JSON,避免误改线上 genesis。
pub fn fresh_genesis_config() -> Result<ChainSpec, String> {
    let wasm = runtime::WASM_BINARY.ok_or_else(|| {
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
    // 不能走 `with_genesis_config_preset_name`。由 genesis crate 接收主 Runtime 的实际
    // 汇总配置类型，在 host 构出完整 JSON，再 `with_genesis_config_patch` 注入。
    let genesis_patch = genesis::config::genesis_config::<runtime::RuntimeGenesisConfig>();

    Ok(crate::chain::ChainSpec::builder(wasm, None)
        .with_name("CitizenChain")
        .with_id("citizenchain")
        .with_chain_type(ChainType::Live)
        .with_protocol_id("citizenchain")
        .with_properties(properties)
        .with_boot_nodes(boot_nodes)
        .with_genesis_config_patch(genesis_patch)
        .build())
}

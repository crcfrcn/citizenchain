use codec::Decode;
use nodecontrol::network::local_network_reader;
use nodedesktop::constants::RPC_RESPONSE_LIMIT_LARGE;
use serde::Serialize;
use serde_json::Value;
use sp_crypto_hashing::twox_128;
use std::collections::HashMap;
use std::collections::HashSet;
use std::time::Duration;
use tauri::AppHandle;

// 网络统计需要查询较多 peer 信息，给予额外 1 秒余量。
const RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(4);

/// 网络总览面板对前端返回的聚合统计结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkOverview {
    pub online_nodes: u64,
    pub nrc_nodes: u64,
    pub prc_nodes: u64,
    pub prb_nodes: u64,
    pub full_nodes: u64,
    pub light_nodes: u64,
    pub warning: Option<String>,
}

fn normalize_peer_id(input: &str) -> Option<String> {
    let v = input.trim();
    if v.is_empty() || v.len() > 128 {
        return None;
    }
    if !v.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    // libp2p Ed25519 PeerId 以 "12D3KooW" 开头，长度通常 >= 46 字符。
    if !v.starts_with("12D3KooW") || v.len() < 46 {
        return None;
    }
    Some(v.to_string())
}

fn rpc_post(method: &str, params: Value) -> Result<Value, String> {
    nodedesktop::rpc::rpc_post(
        method,
        params,
        RPC_REQUEST_TIMEOUT,
        RPC_RESPONSE_LIMIT_LARGE,
    )
}

// 网络统计必须建立在"当前 RPC 确认属于目标链"的前提上，
// 否则宁可降级返回告警，也不要继续产出可能误导的网络数据。
fn ensure_expected_rpc_node() -> Result<(), String> {
    let properties = rpc_post("system_properties", Value::Array(vec![]))?;
    let ss58 = properties
        .get("ss58Format")
        .and_then(|v| {
            if let Some(raw) = v.as_u64() {
                Some(raw)
            } else {
                v.as_str().and_then(|s| s.parse::<u64>().ok())
            }
        })
        .ok_or_else(|| "RPC 节点缺少 ss58Format".to_string())?;
    if ss58 != nodedesktop::constants::EXPECTED_SS58_PREFIX {
        return Err(format!("RPC 链前缀不匹配：expected=2027, got={ss58}"));
    }

    let name = rpc_post("system_name", Value::Array(vec![]))
        .map_err(|err| format!("读取 system_name 失败: {err}"))?
        .as_str()
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    if name.is_empty() {
        return Err("RPC 节点名称为空".to_string());
    }

    nodedesktop::rpc::verify_genesis_hash().map_err(|e| format!("genesis hash 校验失败: {e}"))?;

    Ok(())
}

/// 链上 `CitizenIdentity::CidCount` 的完整存储键。
///
/// pallet 名与 storage 名一旦改动，这里必须同步；单测钉住本函数的字节口径。
fn cid_count_storage_key() -> Vec<u8> {
    [
        twox_128(b"CitizenIdentity").as_slice(),
        twox_128(b"CidCount").as_slice(),
    ]
    .concat()
}

/// 读取链上当前有效 CID 数（挖矿页「轻节点」一格的唯一数据源）。
///
/// 定长单值，一次 `state_getStorage` 读完，不做前缀扫描。
/// 键不存在返回 `Ok(None)`：这是「runtime 升级尚未落链」的真实状态，
/// 不能与「链上确实为 0」混为一谈，更不能压成 0 静默上屏。
fn read_cid_count() -> Result<Option<u64>, String> {
    let key = cid_count_storage_key();
    let value = rpc_post(
        "state_getStorage",
        Value::Array(vec![Value::String(format!("0x{}", hex::encode(key)))]),
    )?;
    let Some(raw_hex) = value.as_str() else {
        return Ok(None);
    };
    let raw = hex::decode(raw_hex.trim_start_matches("0x"))
        .map_err(|e| format!("CidCount 十六进制解码失败: {e}"))?;
    let mut input = raw.as_slice();
    let count = u64::decode(&mut input).map_err(|_| "CidCount SCALE 解码失败".to_string())?;
    if !input.is_empty() {
        return Err("CidCount 非规范编码：存在多余字节".to_string());
    }
    Ok(Some(count))
}

fn extract_light_role(roles_value: &Value) -> bool {
    if let Some(s) = roles_value.as_str() {
        return s.to_ascii_lowercase().contains("light");
    }
    if let Some(arr) = roles_value.as_array() {
        return arr.iter().any(|v| {
            v.as_str()
                .map(|s| s.to_ascii_lowercase().contains("light"))
                .unwrap_or(false)
        });
    }
    false
}

fn collect_local_peers(peers: &Value) -> Result<(HashSet<String>, HashSet<String>, u64), String> {
    let peers = peers.as_array().ok_or("system_peers 返回格式无效")?;
    let mut full = HashSet::new();
    let mut light = HashSet::new();
    let mut invalid = 0;
    for peer in peers {
        let Some(id) = peer
            .get("peerId")
            .and_then(Value::as_str)
            .and_then(normalize_peer_id)
        else {
            invalid += 1;
            continue;
        };
        if peer.get("roles").map(extract_light_role).unwrap_or(false) {
            light.insert(id);
        } else {
            full.insert(id);
        }
    }
    Ok((full, light, invalid))
}

fn count_observed_institutions(
    full: &HashSet<String>,
    light: &HashSet<String>,
    roles: &HashMap<String, String>,
) -> (u64, u64, u64, u64) {
    let (mut nrc, mut prc, mut prb, mut unknown) = (0u64, 0u64, 0u64, 0u64);
    for id in full.iter().chain(light.iter()) {
        match roles.get(id).map(String::as_str) {
            Some("nrc") => nrc = nrc.saturating_add(1),
            Some("prc") => prc = prc.saturating_add(1),
            Some("prb") => prb = prb.saturating_add(1),
            Some(_) => unknown = unknown.saturating_add(1),
            None => {}
        }
    }
    (nrc, prc, prb, unknown)
}

/// 获取网络总览数据（在线节点、治理节点状态、全节点与轻节点统计）。
/// 前端定期轮询此命令；内部通过 spawn_blocking 避免阻塞 Tauri 主线程。
#[tauri::command]
pub async fn get_network_overview(app: AppHandle) -> Result<NetworkOverview, String> {
    tauri::async_runtime::spawn_blocking(move || get_network_overview_blocking(app))
        .await
        .map_err(|e| format!("network overview task failed: {e}"))?
}

fn get_network_overview_blocking(app: AppHandle) -> Result<NetworkOverview, String> {
    // 网络总览是一个"尽量返回"的聚合接口：
    // 只要能确认当前 RPC 属于目标链，就尽量返回当前可观测在线节点和本机角色状态。
    let institution_catalog = nodecontrol::bootnodes::genesis_bootnode_options()?;
    let institution_roles: HashMap<String, String> = institution_catalog
        .iter()
        .map(|n| (n.peer_id.clone(), n.role.clone()))
        .collect();

    let status = nodecontrol::identity::current_status(&app)?;
    if !status.running {
        return Ok(NetworkOverview {
            online_nodes: 0,
            nrc_nodes: 0,
            prc_nodes: 0,
            prb_nodes: 0,
            full_nodes: 0,
            light_nodes: 0,
            warning: None,
        });
    }

    let mut warnings: Vec<String> = Vec::new();
    let rpc_ready = match ensure_expected_rpc_node() {
        Ok(()) => true,
        Err(err) => {
            warnings.push(format!("网络 RPC 校验失败: {err}"));
            false
        }
    };

    // 本机实际观测到的节点按角色分类，以 PeerId 去重。
    //
    // full 集合大小就是对外的全节点数；light 集合只用于把轻节点 peer 排除在全节点之外，
    // 不再直接对外显示（轻节点数改由链上 `CidCount` 提供）。两个集合必须保持互斥：
    // 曾经的写法是用 `online - light` 反推 full，减数与被减数不是同一个集合，本机没直连到的
    // 轻节点每多一个就少算一个全节点，减到负数还被 saturating_sub 压成 0。
    let mut network_full_ids: HashSet<String> = HashSet::new();
    let mut network_light_ids: HashSet<String> = HashSet::new();
    // 机构目录只用于身份匹配；是否计入统计由本机实际观测到的 PeerId 决定。
    let mut local_role_known = false;
    let mut local_is_light = false;
    let mut invalid_peer_count: u64 = 0;

    if rpc_ready {
        match local_network_reader(&app)
            .and_then(|reader| reader.peers())
            .and_then(|peers| collect_local_peers(&peers))
        {
            Ok((full, light, invalid)) => {
                network_full_ids = full;
                network_light_ids = light;
                invalid_peer_count = invalid;
            }
            Err(err) => warnings.push(format!("读取 system_peers 失败: {err}")),
        }
    }

    // 本机身份与本机实际连接到的 peer 合并，按 PeerId 去重。
    let mut local_peer_id: Option<String> = None;
    if rpc_ready {
        match rpc_post("system_localPeerId", Value::Array(vec![])) {
            Ok(v) => match v.as_str().and_then(normalize_peer_id) {
                Some(pid) => local_peer_id = Some(pid),
                None => warnings
                    .push("system_localPeerId 返回为空或格式无效，本机未计入节点数".to_string()),
            },
            Err(err) => warnings.push(format!(
                "读取 system_localPeerId 失败，本机未计入节点数: {err}"
            )),
        }

        match rpc_post("system_nodeRoles", Value::Array(vec![])) {
            Ok(roles) => {
                local_is_light = extract_light_role(&roles);
                local_role_known = true;
            }
            Err(err) => warnings.push(format!(
                "读取 system_nodeRoles 失败，无法判定本机轻/全节点: {err}"
            )),
        }
    }

    match (&local_peer_id, local_role_known) {
        (Some(pid), true) => {
            if local_is_light {
                let _ = network_light_ids.insert(pid.clone());
            } else {
                let _ = network_full_ids.insert(pid.clone());
            }
        }
        (Some(_), false) => {
            // 角色未知时**不猜**。旧实现在这里默认按全节点计入，是为了让
            // `full + light == online` 这个等式成立而做的猜测；同源之后守恒由集合保证，
            // 宁可少计本机一台，也不把它归错类。
            warnings.push("未能判定本机轻/全节点，本机未计入节点数".to_string());
        }
        (None, _) => {}
    }

    if invalid_peer_count > 0 {
        warnings.push(format!("忽略了 {} 条无效 peerId 记录", invalid_peer_count));
    }

    // 全节点取实测 peer 集合；轻节点取链上已注册的有效 CID 数。
    //
    // `network_light_ids` 不再对外显示，但必须继续维护：它的职责变成把带 light 角色的 peer
    // 从全节点集合里剔除。删掉它，CitizenApp 的 smoldot 会被当成全节点计进 full_nodes。
    let full_nodes = network_full_ids.len() as u64;
    let light_nodes = if rpc_ready {
        match read_cid_count() {
            Ok(Some(count)) => count,
            Ok(None) | Err(_) => 0,
        }
    } else {
        0
    };
    let online_nodes = full_nodes.saturating_add(light_nodes);

    // 治理节点仅按本机实际观测到的两个 peer 集合匹配机构身份。
    let (nrc_nodes, prc_nodes, prb_nodes, uncategorized_bootnodes) =
        count_observed_institutions(&network_full_ids, &network_light_ids, &institution_roles);
    if uncategorized_bootnodes > 0 {
        warnings.push(format!(
            "{} 个引导节点角色未命中\u{201c}国家储委会/省储委会/省储行\u{201d}，按全节点口径处理",
            uncategorized_bootnodes
        ));
    }

    Ok(NetworkOverview {
        online_nodes,
        nrc_nodes,
        prc_nodes,
        prb_nodes,
        full_nodes,
        light_nodes,
        warning: if warnings.is_empty() {
            None
        } else {
            Some(warnings.join("；"))
        },
    })
}
#[cfg(test)]
#[path = "../tests/network.rs"]
mod tests;

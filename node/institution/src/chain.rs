//! 从链上读取机构及代表人，构造CID键并派生协议账户。

use crate::types::AccountWithBalance;
use crate::types::InstitutionDetail;
use crate::types::InstitutionProposalPage;
use codec::DecodeAll;
use codec::Encode;
use nodedesktop::constants::RPC_RESPONSE_LIMIT_SMALL;
use primitives::account_derive::institution_kind_by_name;
use primitives::account_derive::AccountKind;
use primitives::account_derive::InstitutionProtocolAccountKind;
use primitives::core_const::SS58_FORMAT;
use serde_json::Value;
use sp_core::ConstU32;
use sp_runtime::AccountId32;
use sp_runtime::BoundedVec;
use std::time::Duration;

const RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

fn rpc_post(method: &str, params: Value) -> Result<Value, String> {
    nodedesktop::rpc::rpc_post(
        method,
        params,
        RPC_REQUEST_TIMEOUT,
        RPC_RESPONSE_LIMIT_SMALL,
    )
}

/// SCALE 编码 cid_number 的 `BoundedVec<u8, ConstU32<32>>` 形式(用作 storage key data)。
///
/// 字段编码:`Compact<u32>(len)` + `bytes`。
fn encode_cid_key_data(cid_number: &str) -> Result<Vec<u8>, String> {
    let raw = cid_number.as_bytes();
    if raw.is_empty() || raw.len() > 32 {
        return Err(format!(
            "cid_number 长度必须在链上 CID_NUMBER_MAX_BYTES 范围内,实际:{}",
            raw.len()
        ));
    }
    let bv: BoundedVec<u8, ConstU32<32>> = raw
        .to_vec()
        .try_into()
        .map_err(|_| "cid_number 超出链上 BoundedVec<u8, 32>".to_string())?;
    Ok(bv.encode())
}

type Account = entitybase::InstitutionAccountInfo<AccountId32, u128, u32>;

/// 按 GMB 协议确定性派生机构主账户 32 字节地址。
fn derive_main_account(cid_number: &str) -> AccountId32 {
    AccountId32::new(
        AccountKind::InstitutionMain {
            cid_number: cid_number.as_bytes(),
        }
        .derive(SS58_FORMAT),
    )
}

/// 按 GMB 协议确定性派生机构费用账户 32 字节地址。
fn derive_fee_account(cid_number: &str) -> AccountId32 {
    AccountId32::new(
        AccountKind::InstitutionFee {
            cid_number: cid_number.as_bytes(),
        }
        .derive(SS58_FORMAT),
    )
}

/// 读取机构治理阈值。
///
/// 真源 = `PublicEntity/PrivateEntity::InstitutionGovernanceThresholds[cid_number]`；
/// 投票引擎只消费该值，不保存第二份机构阈值。
fn fetch_institution_threshold(
    cid_number: &str,
    manage_pallet: &str,
    finalized_hash: &str,
) -> Result<u32, String> {
    let cid_key = encode_cid_key_data(cid_number)?;
    let key =
        nodeclient::query::map_key(manage_pallet, "InstitutionGovernanceThresholds", &cid_key);
    let result = rpc_post(
        "state_getStorage",
        Value::Array(vec![
            Value::String(key),
            Value::String(finalized_hash.to_string()),
        ]),
    )?;
    match result {
        Value::Null => Ok(0),
        Value::String(hex_data) => {
            let clean = hex_data.strip_prefix("0x").unwrap_or(&hex_data);
            let bytes = hex::decode(clean)
                .map_err(|e| format!("InstitutionGovernanceThresholds hex 解码失败:{e}"))?;
            u32::decode_all(&mut &bytes[..])
                .map_err(|e| format!("InstitutionGovernanceThresholds SCALE 解码失败:{e}"))
        }
        _ => Err("state_getStorage 返回格式无效".to_string()),
    }
}

/// 链上查询某机构的多签信息。返回 `None` = 该 cid_number 尚未创建机构(进入创建流程)。
///
/// 数据来源:
/// 1. `PublicEntity/PrivateEntity::Institutions[cid_number]` 取机构身份最小集(按机构码路由 pallet)
/// 2. `state_getKeysPaged` + `…::InstitutionAccounts[cid_number, *]` 取账户列表
/// 3. 每个账户的 `System::Account[address].data.free + reserved` 取链上总余额
/// 4. 主/费账户由 GMB 协议派生地址定位；任职和席位取实体岗位，阈值取 entity
pub(crate) fn fetch_institution_detail(
    cid_number: &str,
) -> Result<Option<InstitutionDetail>, String> {
    // (最终确认块读取):取一次 finalized 快照,机构详情/账户列表/余额/阈值全部钉同一块。
    let finalized_hash = nodeclient::query::fetch_finalized_head()?;
    let Some((owner, inst)) = crate::storage::entity_at(cid_number, &finalized_hash)? else {
        return Ok(None);
    };
    let manage_pallet = owner.name();

    // 主/费账户地址由 (cid_number, 保留名) 确定性派生(与 InstitutionAccounts 中存储的一致)。
    let main_account_id = derive_main_account(cid_number);
    let fee_account_id = derive_fee_account(cid_number);
    let main_account_bytes: [u8; 32] = main_account_id.clone().into();
    let main_account_hex = format!("0x{}", hex::encode(main_account_bytes));
    let fee_account_bytes: [u8; 32] = fee_account_id.clone().into();
    let fee_account_hex = format!("0x{}", hex::encode(fee_account_bytes));

    // 拉机构下所有账户(InstitutionAccounts[cid_number, *] 是 DoubleMap)。
    let accounts = fetch_institution_accounts(cid_number, manage_pallet, &finalized_hash)?;
    let account_count = accounts.len() as u32;

    // 主账户 / 费用账户 / 其它账户分类；协议账户缺失属于链状态错误，不做派生回落。
    let mut main_account_info: Option<AccountWithBalance> = None;
    let mut fee_account_info: Option<AccountWithBalance> = None;
    let mut other_accounts: Vec<AccountWithBalance> = Vec::new();
    for acc in accounts {
        if acc.account_id == main_account_hex {
            main_account_info = Some(acc);
        } else if acc.account_id == fee_account_hex {
            fee_account_info = Some(acc);
        } else {
            other_accounts.push(acc);
        }
    }
    let main_account_info = main_account_info.ok_or_else(|| "机构缺少唯一主账户".to_string())?;
    let fee_account_info = fee_account_info.ok_or_else(|| "机构缺少唯一费用账户".to_string())?;

    let roles = crate::storage::fetch_roles_at(cid_number, &finalized_hash)?
        .ok_or("同块机构岗位状态缺失")?
        .roles;
    let threshold = fetch_institution_threshold(cid_number, manage_pallet, &finalized_hash)?;

    let cid_full_name =
        String::from_utf8(inst.cid_full_name).map_err(|_| "cid_full_name 非 UTF-8".to_string())?;
    // 私权机构链上 cid_full_name 为空时回退展示 cid_number,避免详情页标题空白。
    let cid_full_name = if cid_full_name.trim().is_empty() {
        cid_number.to_string()
    } else {
        cid_full_name
    };

    Ok(Some(InstitutionDetail {
        cid_number: cid_number.to_string(),
        cid_full_name,
        institution_code: inst.institution_code,
        main_account_info,
        fee_account_info,
        other_accounts,
        threshold,
        roles,
        created_at: inst.created_at as u64,
        account_count,
    }))
}

/// 用 `state_getKeysPaged` 列出 `InstitutionAccounts[cid_number, *]` 全部账户名,
/// 然后逐个 `state_getStorage` 拉取账户内容并查链上余额。
fn fetch_institution_accounts(
    cid_number: &str,
    manage_pallet: &str,
    finalized_hash: &str,
) -> Result<Vec<AccountWithBalance>, String> {
    // 第一层 key 哈希器是 Blake2_128Concat,完整 storage key 前缀 =
    //   twox_128(PublicEntity|PrivateEntity) ++ twox_128("InstitutionAccounts")
    //   ++ blake2_128(cid_number_bytes) ++ cid_number_bytes(BoundedVec 编码)
    let cid_key = encode_cid_key_data(cid_number)?;
    let mut cid_prefix_data = Vec::with_capacity(16 + cid_key.len());
    cid_prefix_data.extend_from_slice(&sp_crypto_hashing::blake2_128(&cid_key));
    cid_prefix_data.extend_from_slice(&cid_key);
    let pallet = sp_crypto_hashing::twox_128(manage_pallet.as_bytes());
    let storage = sp_crypto_hashing::twox_128(b"InstitutionAccounts");
    let prefix_hex = format!(
        "0x{}{}{}",
        hex::encode(pallet),
        hex::encode(storage),
        hex::encode(&cid_prefix_data)
    );

    const PAGE: u32 = 100;
    let mut keys: Vec<String> = Vec::new();
    let mut start_key: Option<String> = None;
    loop {
        // (最终确认块读取):key 列举与后续 storage 读取必须钉同一个 finalized
        // 快照哈希;不带 at 参数 = best,分叉窗口内会列出半新半旧的账户集。
        let result = rpc_post(
            "state_getKeysPaged",
            Value::Array(vec![
                Value::String(prefix_hex.clone()),
                Value::Number(serde_json::Number::from(PAGE)),
                match start_key.as_ref() {
                    Some(s) => Value::String(s.clone()),
                    None => Value::Null,
                },
                Value::String(finalized_hash.to_string()),
            ]),
        )?;
        let arr = result
            .as_array()
            .ok_or_else(|| "state_getKeysPaged 返回非数组".to_string())?;
        let n = arr.len();
        for k in arr {
            if let Some(s) = k.as_str() {
                keys.push(s.to_string());
            }
        }
        if n < PAGE as usize {
            break;
        }
        start_key = arr.last().and_then(|v| v.as_str().map(|s| s.to_string()));
        if start_key.is_none() {
            break;
        }
    }

    let mut out: Vec<AccountWithBalance> = Vec::with_capacity(keys.len());
    for key in keys {
        // 解 account_name:key 末尾段 = blake2_128(name_bytes_compact) ++ name_bytes_compact
        // name_bytes_compact = BoundedVec<u8>::encode = Compact(len) ++ bytes
        let value = rpc_post(
            "state_getStorage",
            Value::Array(vec![
                Value::String(key.clone()),
                Value::String(finalized_hash.to_string()),
            ]),
        )?;
        let value_hex = match value {
            Value::Null => continue,
            Value::String(s) => s,
            _ => continue,
        };
        let clean = value_hex.strip_prefix("0x").unwrap_or(&value_hex);
        let bytes = hex::decode(clean).map_err(|e| format!("InstitutionAccounts hex:{e}"))?;
        let acc = match Account::decode_all(&mut &bytes[..]) {
            Ok(v) => v,
            Err(e) => {
                // 单条解码失败不应阻止整页;打 stderr 让运维看见,继续下一条。
                eprintln!(
                    "[clearing-bank] InstitutionAccounts SCALE decode failed key={} err={}",
                    key, e
                );
                continue;
            }
        };
        let acc_name = decode_account_name_from_key(&key, &prefix_hex).unwrap_or_default();
        let account_id_bytes: [u8; 32] = acc.account_id.clone().into();
        // 机构账户列表复用同块总余额入口；读取失败保留错误，只有账户不存在才为零。
        let bal = crate::balance::fetch_balance_at(
            &format!("0x{}", hex::encode(account_id_bytes)),
            Some(finalized_hash),
        )?
        .unwrap_or(0);
        let account_kind = institution_kind_by_name(cid_number.as_bytes(), acc_name.as_bytes())
            .ok_or_else(|| "机构账户名为空".to_string())?;
        let account_kind_label = match account_kind.institution_protocol_kind() {
            Some(InstitutionProtocolAccountKind::Main) => "main",
            Some(InstitutionProtocolAccountKind::Fee) => "fee",
            Some(InstitutionProtocolAccountKind::Stake) => "stake",
            Some(InstitutionProtocolAccountKind::SafetyFund) => "safety_fund",
            Some(InstitutionProtocolAccountKind::He) => "he",
            Some(InstitutionProtocolAccountKind::Clearing) => "clearing",
            Some(InstitutionProtocolAccountKind::FederalCitizenSecurityFund) => {
                "federal_citizen_security_fund"
            }
            None => "named",
        };
        let can_close = account_kind.is_closable_institution_account();
        out.push(AccountWithBalance {
            account_name: acc_name,
            account_id: format!("0x{}", hex::encode(account_id_bytes)),

            balance_fen: bal.to_string(),
            account_kind: account_kind_label.to_string(),
            can_close,
        });
    }

    Ok(out)
}

/// 从完整 storage key 反推第二层 key(account_name 字节)。
fn decode_account_name_from_key(full_key_hex: &str, cid_prefix_hex: &str) -> Option<String> {
    let full = full_key_hex.strip_prefix("0x").unwrap_or(full_key_hex);
    let prefix = cid_prefix_hex.strip_prefix("0x").unwrap_or(cid_prefix_hex);
    if !full.starts_with(prefix) {
        return None;
    }
    // 第二层后缀:blake2_128(account_name_compact) ++ account_name_compact
    let after = &full[prefix.len()..];
    if after.len() < 32 {
        return None;
    }
    let after_bytes = hex::decode(&after[32..]).ok()?;
    if after_bytes.is_empty() {
        return None;
    }
    // Compact<u32>(len) 解码,与 clearing_bank_watcher.rs 的实现一致(单字节模式占多数)
    let first = after_bytes[0];
    let (len, consumed) = match first & 0x03 {
        0 => ((first >> 2) as usize, 1usize),
        1 => {
            if after_bytes.len() < 2 {
                return None;
            }
            let v = ((first as u16) | ((after_bytes[1] as u16) << 8)) >> 2;
            (v as usize, 2)
        }
        2 => {
            if after_bytes.len() < 4 {
                return None;
            }
            let v = (first as u32)
                | ((after_bytes[1] as u32) << 8)
                | ((after_bytes[2] as u32) << 16)
                | ((after_bytes[3] as u32) << 24);
            ((v >> 2) as usize, 4)
        }
        _ => return None,
    };
    if after_bytes.len() < consumed + len {
        return None;
    }
    String::from_utf8(after_bytes[consumed..consumed + len].to_vec()).ok()
}

/// 机构提案列表分页。
///
/// 当前阶段返回空列表占位。提案存储在 `engingcore::Proposals[id]`,
/// 按 cid_number 过滤需要扫描全表 + 对照 ProposalMeta.actor_cid_number，
///
/// 前端 UI 展示"暂无提案"行兜底,未来填充时无需改 UI 结构。
pub(crate) fn fetch_institution_proposals(
    _cid_number: &str,
    _start_id: u128,
    _page_size: u32,
) -> Result<InstitutionProposalPage, String> {
    Ok(InstitutionProposalPage {
        items: Vec::new(),
        has_more: false,
    })
}
#[cfg(test)]
#[path = "../tests/chain.rs"]
mod tests;

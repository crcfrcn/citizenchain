// 清算行机构身份的链上只读查询。
//
//
// - 本文件只读链上机构身份事实,供清算行流程展示:机构最小集、各账户余额、管理员集合、动态阈值。
// - 机构最小集真源 `PublicManage/PrivateManage::Institutions[cid_number]` 只存身份字段;
// - 账户真源为 `InstitutionAccounts[(cid_number, account_name)]`;
// - 管理员与阈值分别按 CID 读取 admins 与 entity 的机构治理阈值真源。
// - 清算行节点声明 `OffchainTransaction::ClearingBankNodes` 走 `offchain::endpoint`,
//   与机构身份只读解耦。

use codec::{Decode, Encode};
use primitives::account_derive::{
    institution_kind_by_name, AccountKind, InstitutionProtocolAccountKind,
};
use primitives::cid::code::InstitutionCode;
use primitives::core_const::SS58_FORMAT;
use serde_json::Value;
use sp_core::ConstU32;
use sp_runtime::{AccountId32, BoundedVec};
use std::time::Duration;

use crate::admins::management::storage as admins_storage;
use crate::governance::chain_query;
use crate::governance::signing::account_id_to_ss58;
use crate::governance::storage_keys;
use crate::shared::{constants::RPC_RESPONSE_LIMIT_SMALL, rpc};

use super::types::{
    AccountWithBalance, InstitutionAdminDisplay, InstitutionDetail, InstitutionProposalPage,
};

const RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

fn rpc_post(method: &str, params: Value) -> Result<Value, String> {
    rpc::rpc_post(
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

// ─── 机构最小集镜像(PublicManage/PrivateManage::Institutions) ──────

/// 链端 `InstitutionInfo<BlockNumber, AccountName, CidNumber, AccountId>` 的 SCALE 镜像。
/// 字段顺序必须与 `runtime/entity/{public,private}-manage/src/institution/types.rs::InstitutionInfo`
/// 严格一致(两 pallet 同形态;Encode/Decode 派生按声明顺序)。
///
/// runtime 实例化的具体类型:
/// - `BlockNumber = u32`(citizenchain runtime)
/// - `AccountName = BoundedVec<u8, ConstU32<128>>`
/// - `CidNumber = BoundedVec<u8, ConstU32<32>>`
/// - `AccountId = AccountId32`
#[derive(Decode)]
#[allow(dead_code)] // SCALE 解码镜像:未读字段只保序,删字段会破坏 Decode 对齐。
struct OnChainInstitution {
    cid_full_name: BoundedVec<u8, ConstU32<128>>,
    cid_short_name: BoundedVec<u8, ConstU32<128>>,
    town_code: BoundedVec<u8, ConstU32<128>>,
    legal_representative: Option<OnChainLegalRepresentative>,
    institution_code: InstitutionCode,
    created_at: u32,
}

#[derive(Decode)]
#[allow(dead_code)] // SCALE 解码镜像:未读字段只保序,删字段会破坏 Decode 对齐。
struct OnChainLegalRepresentative {
    family_name: BoundedVec<u8, ConstU32<128>>,
    given_name: BoundedVec<u8, ConstU32<128>>,
    cid_number: BoundedVec<u8, ConstU32<32>>,
    account: AccountId32,
}

/// 链端 `InstitutionAccountInfo<AccountId, Balance, BlockNumber>` 镜像。
#[derive(Decode)]
#[allow(dead_code)] // SCALE 解码镜像:未读字段只保序,删字段会破坏 Decode 对齐。
struct OnChainInstitutionAccount {
    address: AccountId32,
    initial_balance: u128,
    created_at: u32,
}

/// `frame_system::AccountInfo` 头部 16 字节(nonce/consumers/providers/sufficients
/// 4 个 u32),紧接 16 字节 `data.free` u128。
const ACCOUNT_INFO_HEADER_LEN: usize = 16;
const ACCOUNT_INFO_FREE_LEN: usize = 16;

/// 把"分"格式化成 `xxx.xx`(沿用 chain/balance/handler.rs 同款约定)。
fn format_yuan(min_units: u128) -> String {
    let yuan = min_units / 100;
    let cents = (min_units % 100) as u8;
    format!("{}.{:02}", yuan, cents)
}

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

/// 用 RPC `state_getStorage` 拉单账户的 free 余额(分)。
/// 不存在 → 返回 0。
fn fetch_account_free_balance(account: &AccountId32, finalized_hash: &str) -> Result<u128, String> {
    let raw: [u8; 32] = (*account).clone().into();
    // System.Account 用 Blake2_128Concat 哈希器,key = blake2_128(account) ++ account
    let mut hashed = Vec::with_capacity(48);
    hashed.extend_from_slice(&storage_keys::blake2_128(&raw));
    hashed.extend_from_slice(&raw);
    let key = format!(
        "0x{}{}{}",
        hex::encode(storage_keys::twox_128(b"System")),
        hex::encode(storage_keys::twox_128(b"Account")),
        hex::encode(hashed)
    );
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
            let bytes =
                hex::decode(clean).map_err(|e| format!("System.Account hex 解码失败:{e}"))?;
            let need = ACCOUNT_INFO_HEADER_LEN + ACCOUNT_INFO_FREE_LEN;
            if bytes.len() < need {
                return Err(format!(
                    "AccountInfo bytes too short: got {}, need >= {need}",
                    bytes.len()
                ));
            }
            let mut buf = [0u8; 16];
            buf.copy_from_slice(
                &bytes[ACCOUNT_INFO_HEADER_LEN..ACCOUNT_INFO_HEADER_LEN + ACCOUNT_INFO_FREE_LEN],
            );
            Ok(u128::from_le_bytes(buf))
        }
        _ => Err("state_getStorage 返回格式无效".to_string()),
    }
}

/// 读取机构管理员账户集合（岗位/任职需另查 entity）及人数。
///
/// 真源 = 机构码对应管理员 pallet 的 `AdminAccounts[cid_number]`,值为钱包账户列表;
/// 机构码不匹配视为数据不一致,降级为空集合。
fn fetch_admin_set(
    institution_code: &InstitutionCode,
    cid_number: &str,
    finalized_hash: &str,
) -> Result<(Vec<InstitutionAdminDisplay>, u32), String> {
    let Some(state) = admins_storage::fetch_institution_admins_for_code_at(
        cid_number,
        *institution_code,
        finalized_hash,
    )?
    else {
        return Ok((Vec::new(), 0));
    };
    if &state.institution_code != institution_code {
        return Ok((Vec::new(), 0));
    }
    let admins_len = state.admins.len() as u32;
    Ok((state.admins, admins_len))
}

/// 读取机构治理阈值。
///
/// 真源 = `PublicManage/PrivateManage::InstitutionGovernanceThresholds[cid_number]`；
/// 投票引擎只消费该值，不保存第二份机构阈值。
fn fetch_institution_threshold(
    cid_number: &str,
    manage_pallet: &str,
    finalized_hash: &str,
) -> Result<u32, String> {
    let cid_key = encode_cid_key_data(cid_number)?;
    let key = storage_keys::map_key(manage_pallet, "InstitutionGovernanceThresholds", &cid_key);
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
            u32::decode(&mut &bytes[..])
                .map_err(|e| format!("InstitutionGovernanceThresholds SCALE 解码失败:{e}"))
        }
        _ => Err("state_getStorage 返回格式无效".to_string()),
    }
}

/// 机构生命周期 storage 所在 pallet 名:私权法人码→`PrivateManage`,否则→`PublicManage`。
///
/// 链端机构管理已拆分 `PublicManage`(idx30)/`PrivateManage`(idx31),storage 名
/// (`Institutions`/`InstitutionAccounts`)不变但前缀(twox_128(pallet 名))随之变;前缀由
/// cid_number 派生的 institution_code 经 `is_private_legal_code` 路由(与链端单源一致),
/// 取代已删的 `OrganizationManage`。
fn institution_manage_pallet(cid_number: &str) -> &'static str {
    match primitives::cid::code::institution_code_from_cid_number(cid_number) {
        Some(code) if primitives::cid::code::is_private_legal_code(&code) => "PrivateManage",
        _ => "PublicManage",
    }
}

/// 链上查询某机构的多签信息。返回 `None` = 该 cid_number 尚未创建机构(进入创建流程)。
///
/// 数据来源:
/// 1. `PublicManage/PrivateManage::Institutions[cid_number]` 取机构身份最小集(按机构码路由 pallet)
/// 2. `state_getKeysPaged` + `…::InstitutionAccounts[cid_number, *]` 取账户列表
/// 3. 每个账户的 `System::Account[address].data.free` 取链上余额
/// 4. 主/费账户由 GMB 协议派生地址定位；管理员集合取 admins，阈值取 entity
pub fn fetch_institution_detail(cid_number: &str) -> Result<Option<InstitutionDetail>, String> {
    // (ADR-017):取一次 finalized 快照,机构详情/账户列表/余额/阈值全部钉同一块。
    let finalized_hash = chain_query::fetch_finalized_head()?;
    let key_data = encode_cid_key_data(cid_number)?;
    // 机构生命周期已拆 PublicManage/PrivateManage,按机构码路由 storage 前缀。
    let manage_pallet = institution_manage_pallet(cid_number);
    let key = storage_keys::map_key(manage_pallet, "Institutions", &key_data);
    let result = rpc_post(
        "state_getStorage",
        Value::Array(vec![
            Value::String(key),
            Value::String(finalized_hash.clone()),
        ]),
    )?;
    let raw = match result {
        Value::Null => return Ok(None),
        Value::String(s) => s,
        _ => return Err("state_getStorage 返回格式无效".to_string()),
    };
    let clean = raw.strip_prefix("0x").unwrap_or(&raw);
    let bytes = hex::decode(clean).map_err(|e| format!("Institutions hex 解码失败:{e}"))?;
    let inst = OnChainInstitution::decode(&mut &bytes[..])
        .map_err(|e| format!("Institutions SCALE 解码失败:{e}"))?;

    // 主/费账户地址由 (cid_number, 保留名) 确定性派生(与 InstitutionAccounts 中存储的一致)。
    let main_account_id = derive_main_account(cid_number);
    let fee_account_id = derive_fee_account(cid_number);
    let main_account_bytes: [u8; 32] = main_account_id.clone().into();
    let main_ss58_address = account_id_to_ss58(&main_account_bytes).unwrap_or_default();
    let fee_account_bytes: [u8; 32] = fee_account_id.clone().into();
    let fee_ss58_address = account_id_to_ss58(&fee_account_bytes).unwrap_or_default();

    // 拉机构下所有账户(InstitutionAccounts[cid_number, *] 是 DoubleMap)。
    let accounts = fetch_institution_accounts(cid_number, manage_pallet, &finalized_hash)?;
    let account_count = accounts.len() as u32;

    // 主账户 / 费用账户 / 其它账户分类；协议账户缺失属于链状态错误，不做派生回落。
    let mut main_account_info: Option<AccountWithBalance> = None;
    let mut fee_account_info: Option<AccountWithBalance> = None;
    let mut other_accounts: Vec<AccountWithBalance> = Vec::new();
    for acc in accounts {
        if acc.ss58_address == main_ss58_address {
            main_account_info = Some(acc);
        } else if acc.ss58_address == fee_ss58_address {
            fee_account_info = Some(acc);
        } else {
            other_accounts.push(acc);
        }
    }
    let main_account_info = main_account_info.ok_or_else(|| "机构缺少唯一主账户".to_string())?;
    let fee_account_info = fee_account_info.ok_or_else(|| "机构缺少唯一费用账户".to_string())?;

    let (admins, admins_len) =
        fetch_admin_set(&inst.institution_code, cid_number, &finalized_hash)?;
    let threshold = fetch_institution_threshold(cid_number, manage_pallet, &finalized_hash)?;

    let cid_full_name = String::from_utf8(inst.cid_full_name.into_inner())
        .map_err(|_| "cid_full_name 非 UTF-8".to_string())?;
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
        admins_len,
        threshold,
        admins,
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
    //   twox_128(PublicManage|PrivateManage) ++ twox_128("InstitutionAccounts")
    //   ++ blake2_128(cid_number_bytes) ++ cid_number_bytes(BoundedVec 编码)
    let cid_key = encode_cid_key_data(cid_number)?;
    let mut cid_prefix_data = Vec::with_capacity(16 + cid_key.len());
    cid_prefix_data.extend_from_slice(&storage_keys::blake2_128(&cid_key));
    cid_prefix_data.extend_from_slice(&cid_key);
    let pallet = storage_keys::twox_128(manage_pallet.as_bytes());
    let storage = storage_keys::twox_128(b"InstitutionAccounts");
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
        // (ADR-017):key 列举与后续 storage 读取必须钉同一个 finalized
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
        let acc = match OnChainInstitutionAccount::decode(&mut &bytes[..]) {
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
        let account_id_bytes: [u8; 32] = acc.address.clone().into();
        let bal = fetch_account_free_balance(&acc.address, finalized_hash).unwrap_or(0);
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
            ss58_address: account_id_to_ss58(&account_id_bytes).unwrap_or_default(),
            balance_min_units: bal.to_string(),
            balance_text: format_yuan(bal),
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
/// 当前阶段返回空列表占位。提案存储在 `votingengine::Proposals[id]`,
/// 按 cid_number 过滤需要扫描全表 + 对照 ProposalMeta.actor_cid_number，
/// 实现略显重,放 follow-up 任务卡。
///
/// 前端 UI 展示"暂无提案"行兜底,未来填充时无需改 UI 结构。
pub fn fetch_institution_proposals(
    _cid_number: &str,
    _start_id: u64,
    _page_size: u32,
) -> Result<InstitutionProposalPage, String> {
    Ok(InstitutionProposalPage {
        items: Vec::new(),
        has_more: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_cid_key_data_round_trip() {
        let raw = "LN001-NRC0G-944805165-2026";
        let encoded = encode_cid_key_data(raw).unwrap();
        // Compact<u32> 长度前缀(单字节模式 raw.len() < 64)+ raw 字节
        assert_eq!(encoded[0], (raw.len() as u8) << 2);
        assert_eq!(&encoded[1..], raw.as_bytes());
    }

    #[test]
    fn empty_cid_rejected() {
        let err = encode_cid_key_data("").unwrap_err();
        assert!(err.contains("长度"));
    }

    #[test]
    fn over_long_cid_rejected() {
        let s = "a".repeat(97);
        let err = encode_cid_key_data(&s).unwrap_err();
        assert!(err.contains("长度"));
    }

    #[test]
    fn main_and_fee_accounts_differ() {
        let cid = "LN001-NRC0G-944805165-2026";
        assert_ne!(derive_main_account(cid), derive_fee_account(cid));
    }
}

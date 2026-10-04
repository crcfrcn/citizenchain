//! 链上事件解析器。
//!
//! 使用 subxt 动态 API 解码区块事件，匹配所有余额变动事件，
//! 转换为 `TxRecordInsert` 写入数据库。

use chrono::{DateTime, TimeZone, Utc};
use subxt::events::{EventDetails, Phase};
use subxt::ext::scale_value::{At, Composite, Value};
use subxt::PolkadotConfig;
use tracing::warn;

use super::db::TxRecordInsert;

/// 将链上 32 字节 AccountId 规范化为全仓唯一账户标识。
fn account_id_text(bytes: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(bytes))
}

/// 从 subxt scale_value::Value 提取 32 字节 AccountId。
///
/// AccountId 在 scale-value 中通常表示为一个包含 32 个 u8 primitive 的 unnamed composite。
fn extract_account_id<T>(val: &Value<T>) -> Option<[u8; 32]> {
    // 尝试从 composite 中提取 32 字节
    match &val.value {
        subxt::ext::scale_value::ValueDef::Composite(composite) => {
            extract_bytes_from_composite(composite)
        }
        _ => None,
    }
}

/// 从 Composite 提取 32 字节。
fn extract_bytes_from_composite<T>(composite: &Composite<T>) -> Option<[u8; 32]> {
    let mut bytes = [0u8; 32];
    let values: Vec<_> = composite.values().collect();
    if values.len() != 32 {
        return None;
    }
    for (i, val) in values.iter().enumerate() {
        bytes[i] = val.as_u128()? as u8;
    }
    Some(bytes)
}

/// 从 subxt Value 提取 u128 金额。
fn extract_balance<T>(val: &Value<T>) -> Option<u128> {
    val.as_u128()
}

/// 从事件字段(BoundedVec<u8> = u8 序列 Composite)提取 CID 号字符串。
fn extract_cid_number<T>(val: &Value<T>) -> Option<String> {
    match &val.value {
        subxt::ext::scale_value::ValueDef::Composite(composite) => {
            let bytes: Option<Vec<u8>> = composite
                .values()
                .map(|v| v.as_u128().map(|n| n as u8))
                .collect();
            String::from_utf8(bytes?).ok()
        }
        _ => None,
    }
}

/// 扫描一个区块的事件,收集需要投影的公民/私权机构 CID(供 indexer 增量投影)。
///
/// `CitizenIdentity` 事件的 `cid_number` = 目标公民;`PrivateManage` 事件的 `cid_number` =
/// 目标机构。返回去重后的 (公民 CID, 私权机构 CID)。作用域过滤由投影层按 residence/CID 市码判定。
pub(crate) fn collect_entity_projection_cids(
    events: &subxt::events::Events<PolkadotConfig>,
) -> (Vec<String>, Vec<String>) {
    let mut citizen_cids = Vec::new();
    let mut institution_cids = Vec::new();
    for event_result in events.iter() {
        let Ok(event) = event_result else {
            continue;
        };
        let target = match event.pallet_name() {
            "CitizenIdentity" => &mut citizen_cids,
            "PrivateManage" => &mut institution_cids,
            _ => continue,
        };
        if let Ok(fields) = event.field_values() {
            if let Some(cid) = fields.at("cid_number").and_then(extract_cid_number) {
                target.push(cid);
            }
        }
    }
    citizen_cids.sort();
    citizen_cids.dedup();
    institution_cids.sort();
    institution_cids.dedup();
    (citizen_cids, institution_cids)
}

/// 将 u128 余额（分）转为 i64。超过 i64::MAX 截断（实际不会发生）。
fn balance_to_i64(amount: u128) -> i64 {
    amount.min(i64::MAX as u128) as i64
}

/// 解析一个区块的所有事件，返回需要写入的交易记录。
pub(crate) fn parse_block_events(
    events: &subxt::events::Events<PolkadotConfig>,
    block_number: i64,
    block_timestamp_ms: Option<u64>,
) -> Vec<TxRecordInsert> {
    let block_ts = block_timestamp_ms.and_then(|ms| Utc.timestamp_millis_opt(ms as i64).single());

    let mut records = Vec::new();

    for (event_index, event_result) in events.iter().enumerate() {
        let event = match event_result {
            Ok(e) => e,
            Err(err) => {
                warn!(
                    block = block_number,
                    event_index, error = %err,
                    "failed to decode event, skipping"
                );
                continue;
            }
        };

        let pallet = event.pallet_name();
        let variant = event.variant_name();
        let ext_idx = match event.phase() {
            Phase::ApplyExtrinsic(i) => Some(i as i16),
            _ => None,
        };

        if let Some(mut rec) = match_event(pallet, variant, &event, block_number, ext_idx, block_ts)
        {
            rec.event_index = event_index as i16;
            records.push(rec);
        }
    }

    records
}

/// 匹配单个事件，返回 Some(TxRecordInsert) 如果是余额变动事件。
fn match_event(
    pallet: &str,
    variant: &str,
    event: &EventDetails<PolkadotConfig>,
    block_number: i64,
    extrinsic_index: Option<i16>,
    block_ts: Option<DateTime<Utc>>,
) -> Option<TxRecordInsert> {
    let fields = event.field_values().ok()?;

    match (pallet, variant) {
        // ─── pallet_balances (index 2) ──────────────────────────────
        ("Balances", "Transfer") => {
            let from = fields.at("from").and_then(extract_account_id)?;
            let to = fields.at("to").and_then(extract_account_id)?;
            let amount = fields.at("amount").and_then(extract_balance)?;
            Some(TxRecordInsert {
                block_number,
                extrinsic_index,
                event_index: 0,
                tx_type: "transfer",
                sender_account_id: Some(account_id_text(&from)),
                recipient_account_id: Some(account_id_text(&to)),
                amount_fen: balance_to_i64(amount),
                fee_fen: None,
                block_timestamp: block_ts,
            })
        }
        ("Balances", "Withdraw") => {
            let who = fields.at("who").and_then(extract_account_id)?;
            let amount = fields.at("amount").and_then(extract_balance)?;
            Some(TxRecordInsert {
                block_number,
                extrinsic_index,
                event_index: 0,
                tx_type: "fee_withdraw",
                sender_account_id: Some(account_id_text(&who)),
                recipient_account_id: None,
                amount_fen: balance_to_i64(amount),
                fee_fen: None,
                block_timestamp: block_ts,
            })
        }
        ("Balances", "Deposit") => {
            let who = fields.at("who").and_then(extract_account_id)?;
            let amount = fields.at("amount").and_then(extract_balance)?;
            Some(TxRecordInsert {
                block_number,
                extrinsic_index,
                event_index: 0,
                tx_type: "fee_deposit",
                sender_account_id: None,
                recipient_account_id: Some(account_id_text(&who)),
                amount_fen: balance_to_i64(amount),
                fee_fen: None,
                block_timestamp: block_ts,
            })
        }

        // ─── fullnode_issuance (index 6) ──────────────────────────
        ("FullnodeIssuance", "FullnodeIssuanceIssued") => {
            let wallet = fields.at("wallet").and_then(extract_account_id)?;
            let amount = fields.at("amount").and_then(extract_balance)?;
            Some(TxRecordInsert {
                block_number,
                extrinsic_index,
                event_index: 0,
                tx_type: "block_reward",
                sender_account_id: None,
                recipient_account_id: Some(account_id_text(&wallet)),
                amount_fen: balance_to_i64(amount),
                fee_fen: None,
                block_timestamp: block_ts,
            })
        }

        // ─── provincialbank_interest (index 5) ─────────────────────
        ("ProvincialBankInterest", "ProvincialBankInterestMinted") => {
            let account = fields.at("account").and_then(extract_account_id)?;
            let amount = fields.at("amount").and_then(extract_balance)?;
            Some(TxRecordInsert {
                block_number,
                extrinsic_index,
                event_index: 0,
                tx_type: "bank_interest",
                sender_account_id: None,
                recipient_account_id: Some(account_id_text(&account)),
                amount_fen: balance_to_i64(amount),
                fee_fen: None,
                block_timestamp: block_ts,
            })
        }

        // ─── resolution_issuance (index 8) ──────────────────────
        ("ResolutionIssuance", "ResolutionIssuanceExecuted") => {
            let total = fields.at("total_amount").and_then(extract_balance)?;
            Some(TxRecordInsert {
                block_number,
                extrinsic_index,
                event_index: 0,
                tx_type: "gov_issuance",
                sender_account_id: None,
                recipient_account_id: None,
                amount_fen: balance_to_i64(total),
                fee_fen: None,
                block_timestamp: block_ts,
            })
        }

        // ─── citizen_issuance (index 11) ───────────────────
        ("CitizenIssuance", "CertificationRewardIssued") => {
            let who = fields.at("who").and_then(extract_account_id)?;
            let reward = fields.at("reward").and_then(extract_balance)?;
            Some(TxRecordInsert {
                block_number,
                extrinsic_index,
                event_index: 0,
                tx_type: "lightnode_reward",
                sender_account_id: None,
                recipient_account_id: Some(account_id_text(&who)),
                amount_fen: balance_to_i64(reward),
                fee_fen: None,
                block_timestamp: block_ts,
            })
        }

        // ─── multisig_transfer (index 19) ────────────────────────
        ("MultisigTransfer", "TransferExecuted") => {
            let beneficiary = fields.at("beneficiary").and_then(extract_account_id)?;
            let amount = fields.at("amount").and_then(extract_balance)?;
            let fee = fields.at("fee").and_then(extract_balance);
            Some(TxRecordInsert {
                block_number,
                extrinsic_index,
                event_index: 0,
                tx_type: "proposal_transfer",
                sender_account_id: None,
                recipient_account_id: Some(account_id_text(&beneficiary)),
                amount_fen: balance_to_i64(amount),
                fee_fen: fee.map(balance_to_i64),
                block_timestamp: block_ts,
            })
        }

        // ─── personal_manage (index 7) ──────────────────────────
        // MultisigCreated/MultisigClosed 由 PersonalManage 发射;
        // 机构多签的创建/关闭事件名为 InstitutionCreated/InstitutionClosed,本 indexer 不收录。
        ("PersonalManage", "MultisigCreated") => {
            let multisig = fields.at("account").and_then(extract_account_id)?;
            let creator = fields.at("creator").and_then(extract_account_id)?;
            let amount = fields.at("amount").and_then(extract_balance)?;
            let fee = fields.at("fee").and_then(extract_balance);
            Some(TxRecordInsert {
                block_number,
                extrinsic_index,
                event_index: 0,
                tx_type: "institution_multisig_create",
                sender_account_id: Some(account_id_text(&creator)),
                recipient_account_id: Some(account_id_text(&multisig)),
                amount_fen: balance_to_i64(amount),
                fee_fen: fee.map(balance_to_i64),
                block_timestamp: block_ts,
            })
        }
        ("PersonalManage", "MultisigClosed") => {
            let multisig = fields.at("account").and_then(extract_account_id)?;
            let beneficiary = fields.at("beneficiary").and_then(extract_account_id)?;
            let amount = fields.at("amount").and_then(extract_balance)?;
            let fee = fields.at("fee").and_then(extract_balance);
            Some(TxRecordInsert {
                block_number,
                extrinsic_index,
                event_index: 0,
                tx_type: "institution_multisig_close",
                sender_account_id: Some(account_id_text(&multisig)),
                recipient_account_id: Some(account_id_text(&beneficiary)),
                amount_fen: balance_to_i64(amount),
                fee_fen: fee.map(balance_to_i64),
                block_timestamp: block_ts,
            })
        }

        // ─── resolution_destroy (index 14) ───────────────────────
        ("ResolutionDestroy", "DestroyExecuted") => {
            let amount = fields.at("amount").and_then(extract_balance)?;
            Some(TxRecordInsert {
                block_number,
                extrinsic_index,
                event_index: 0,
                tx_type: "fund_destroy",
                sender_account_id: None,
                recipient_account_id: None,
                amount_fen: balance_to_i64(amount),
                fee_fen: None,
                block_timestamp: block_ts,
            })
        }

        _ => None,
    }
}

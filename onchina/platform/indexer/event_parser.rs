//! 链上事件解析器。
//!
//! 使用 subxt 动态 API 解码区块事件，匹配所有余额变动事件，
//! 转换为 `TxRecordInsert` 写入数据库。

use chrono::{DateTime, TimeZone, Utc};
use codec::Decode;
use subxt::events::{EventDetails, Phase};
use subxt::ext::scale_value::{At, Composite, Value};
use subxt::PolkadotConfig;

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
    byte_sequence(composite)?.try_into().ok()
}

/// scale-value 保留 AccountId32/BoundedVec 单字段包装；只解包真实单字段 composite。
fn byte_sequence<T>(composite: &Composite<T>) -> Option<Vec<u8>> {
    let values: Vec<_> = composite.values().collect();
    if values.len() == 1 {
        if let subxt::ext::scale_value::ValueDef::Composite(inner) = &values[0].value {
            return byte_sequence(inner);
        }
    }
    values
        .into_iter()
        .map(|value| u8::try_from(value.as_u128()?).ok())
        .collect()
}

/// 从 subxt Value 提取 u128 金额。
fn extract_balance<T>(val: &Value<T>) -> Option<u128> {
    val.as_u128()
}

/// 从事件字段(BoundedVec<u8> = u8 序列 Composite)提取 CID 号字符串。
fn extract_cid_number<T>(val: &Value<T>) -> Option<String> {
    match &val.value {
        subxt::ext::scale_value::ValueDef::Composite(composite) => {
            String::from_utf8(byte_sequence(composite)?).ok()
        }
        _ => None,
    }
}

/// 扫描一个区块的事件,收集需要投影的公民/私权机构 CID(供 indexer 增量投影)。
///
/// `CitizenIdentity` 事件的 `cid_number` = 目标公民;`PrivateManage` 事件的 `cid_number` =
/// 目标机构。返回去重后的 (公民 CID, 私权机构 CID)。作用域过滤由投影层按 residence/CID 市码判定。
pub fn collect_entity_projection_cids(
    events: &subxt::events::Events<PolkadotConfig>,
) -> Result<(Vec<String>, Vec<String>), String> {
    let mut citizen_cids = Vec::new();
    let mut institution_cids = Vec::new();
    let mut decoded = 0u32;
    // 从真实字节解码长度前缀；Subxt的零事件回退不能吞掉损坏前缀。
    let mut cursor = events.bytes();
    if !cursor.is_empty() {
        codec::Compact::<u32>::decode(&mut cursor)
            .map_err(|e| format!("decode event count failed: {e}"))?;
    }
    let mut consumed = events.bytes().len() - cursor.len();
    for event_result in events.iter() {
        decoded += 1;
        let event = event_result.map_err(|e| format!("decode entity event failed: {e}"))?;
        consumed += event.bytes().len();
        let target = match event.pallet_name() {
            "CitizenIdentity" => &mut citizen_cids,
            "PrivateManage" => &mut institution_cids,
            _ => continue,
        };
        let fields = event
            .field_values()
            .map_err(|e| format!("decode entity fields failed: {e}"))?;
        // 与实体无关的事件可以没有 CID；存在 CID 字段却不可解码必须阻止游标前移。
        if let Some(value) = fields.at("cid_number") {
            target.push(extract_cid_number(value).ok_or("invalid entity CID event field")?);
        }
    }
    if decoded != events.len() || consumed != events.bytes().len() {
        return Err("entity event count does not match encoded vector".into());
    }
    citizen_cids.sort();
    citizen_cids.dedup();
    institution_cids.sort();
    institution_cids.dedup();
    Ok((citizen_cids, institution_cids))
}

/// 数据库金额必须可无损表示；越界视为解析错误，禁止截断金额后推进进度。
fn balance_to_i64(amount: u128) -> Option<i64> {
    i64::try_from(amount).ok()
}

/// 解析一个区块的所有事件，返回需要写入的交易记录。
pub(crate) fn parse_block_events(
    events: &subxt::events::Events<PolkadotConfig>,
    block_number: i64,
    block_timestamp_ms: Option<u64>,
) -> Result<Vec<TxRecordInsert>, String> {
    let block_ts = block_timestamp_ms
        .map(|ms| {
            let ms = i64::try_from(ms).map_err(|_| "block timestamp exceeds database capacity")?;
            Utc.timestamp_millis_opt(ms)
                .single()
                .ok_or("invalid block timestamp")
        })
        .transpose()?;

    let mut records = Vec::new();

    let mut decoded = 0u32;
    // 从真实字节解码长度前缀；Subxt的零事件回退不能吞掉损坏前缀。
    let mut cursor = events.bytes();
    if !cursor.is_empty() {
        codec::Compact::<u32>::decode(&mut cursor)
            .map_err(|e| format!("decode event count failed: {e}"))?;
    }
    let mut consumed = events.bytes().len() - cursor.len();
    for (event_index, event_result) in events.iter().enumerate() {
        decoded += 1;
        let event = event_result.map_err(|error| {
            format!(
                "decode indexed event failed at block {block_number} event {event_index}: {error}"
            )
        })?;

        consumed += event.bytes().len();
        let pallet = event.pallet_name();
        let variant = event.variant_name();
        let ext_idx = match event.phase() {
            Phase::ApplyExtrinsic(i) => {
                Some(i16::try_from(i).map_err(|_| "extrinsic index exceeds database capacity")?)
            }
            _ => None,
        };

        if let Some(mut rec) =
            match_event(pallet, variant, &event, block_number, ext_idx, block_ts)?
        {
            rec.event_index =
                i16::try_from(event_index).map_err(|_| "event index exceeds database capacity")?;
            records.push(rec);
        }
    }

    if decoded != events.len() || consumed != events.bytes().len() {
        return Err("indexed event count does not match encoded vector".into());
    }
    Ok(records)
}

/// 匹配单个事件，返回 Some(TxRecordInsert) 如果是余额变动事件。
fn match_event(
    pallet: &str,
    variant: &str,
    event: &EventDetails<PolkadotConfig>,
    block_number: i64,
    extrinsic_index: Option<i16>,
    block_ts: Option<DateTime<Utc>>,
) -> Result<Option<TxRecordInsert>, String> {
    // 无关事件无需解析字段；已登记事件的字段缺失、类型错误或越界必须失败。
    if !matches!(
        (pallet, variant),
        ("Balances", "Transfer" | "Withdraw" | "Deposit")
            | ("FullnodeIssuance", "FullnodeIssuanceIssued")
            | ("ProvincialBankInterest", "ProvincialBankInterestMinted")
            | ("ResolutionIssuance", "ResolutionIssuanceExecuted")
            | ("CitizenIssuance", "CertificationRewardIssued")
            | ("MultisigTransfer", "TransferExecuted")
            | ("PersonalManage", "PersonalCreated" | "PersonalClosed")
            | ("ResolutionDestroy", "DestroyExecuted")
    ) {
        return Ok(None);
    }
    let fields = event
        .field_values()
        .map_err(|error| format!("decode {pallet}::{variant} fields failed: {error}"))?;
    let record = (|| -> Option<TxRecordInsert> {
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
                    amount_fen: balance_to_i64(amount)?,
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
                    amount_fen: balance_to_i64(amount)?,
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
                    amount_fen: balance_to_i64(amount)?,
                    fee_fen: None,
                    block_timestamp: block_ts,
                })
            }

            // ─── fullnodeissue (index 6) ──────────────────────────
            ("FullnodeIssuance", "FullnodeIssuanceIssued") => {
                let wallet = fields
                    .at("reward_account_id")
                    .and_then(extract_account_id)?;
                let amount = fields.at("amount").and_then(extract_balance)?;
                Some(TxRecordInsert {
                    block_number,
                    extrinsic_index,
                    event_index: 0,
                    tx_type: "block_reward",
                    sender_account_id: None,
                    recipient_account_id: Some(account_id_text(&wallet)),
                    amount_fen: balance_to_i64(amount)?,
                    fee_fen: None,
                    block_timestamp: block_ts,
                })
            }

            // ─── prbbankissue (index 5) ─────────────────────
            ("ProvincialBankInterest", "ProvincialBankInterestMinted") => {
                let account = fields.at("account_id").and_then(extract_account_id)?;
                let amount = fields.at("amount").and_then(extract_balance)?;
                Some(TxRecordInsert {
                    block_number,
                    extrinsic_index,
                    event_index: 0,
                    tx_type: "bank_interest",
                    sender_account_id: None,
                    recipient_account_id: Some(account_id_text(&account)),
                    amount_fen: balance_to_i64(amount)?,
                    fee_fen: None,
                    block_timestamp: block_ts,
                })
            }

            // ─── resolutionissue (index 8) ──────────────────────
            ("ResolutionIssuance", "ResolutionIssuanceExecuted") => {
                let total = fields.at("total_amount").and_then(extract_balance)?;
                Some(TxRecordInsert {
                    block_number,
                    extrinsic_index,
                    event_index: 0,
                    tx_type: "gov_issuance",
                    sender_account_id: None,
                    recipient_account_id: None,
                    amount_fen: balance_to_i64(total)?,
                    fee_fen: None,
                    block_timestamp: block_ts,
                })
            }

            // ─── citizenissue (index 11) ───────────────────
            ("CitizenIssuance", "CertificationRewardIssued") => {
                let who = fields.at("account_id").and_then(extract_account_id)?;
                let reward = fields.at("reward").and_then(extract_balance)?;
                Some(TxRecordInsert {
                    block_number,
                    extrinsic_index,
                    event_index: 0,
                    tx_type: "lightnode_reward",
                    sender_account_id: None,
                    recipient_account_id: Some(account_id_text(&who)),
                    amount_fen: balance_to_i64(reward)?,
                    fee_fen: None,
                    block_timestamp: block_ts,
                })
            }

            // ─── multisig_transfer (index 19) ────────────────────────
            ("MultisigTransfer", "TransferExecuted") => {
                let beneficiary = fields
                    .at("beneficiary_account_id")
                    .and_then(extract_account_id)?;
                let amount = fields.at("amount").and_then(extract_balance)?;
                let fee = fields.at("fee").and_then(extract_balance)?;
                Some(TxRecordInsert {
                    block_number,
                    extrinsic_index,
                    event_index: 0,
                    tx_type: "proposal_transfer",
                    sender_account_id: None,
                    recipient_account_id: Some(account_id_text(&beneficiary)),
                    amount_fen: balance_to_i64(amount)?,
                    fee_fen: Some(balance_to_i64(fee)?),
                    block_timestamp: block_ts,
                })
            }

            // ─── personalentity (index 7) ──────────────────────────
            // 个人多签创建/关闭按 Runtime 的真实事件名读取;
            // 机构多签的创建/关闭事件名为 InstitutionCreated/InstitutionClosed,本 indexer 不收录。
            ("PersonalManage", "PersonalCreated") => {
                let multisig = fields.at("account_id").and_then(extract_account_id)?;
                let creator = fields
                    .at("creator_account_id")
                    .and_then(extract_account_id)?;
                let amount = fields.at("amount").and_then(extract_balance)?;
                let fee = fields.at("fee").and_then(extract_balance)?;
                Some(TxRecordInsert {
                    block_number,
                    extrinsic_index,
                    event_index: 0,
                    tx_type: "institution_multisig_create",
                    sender_account_id: Some(account_id_text(&creator)),
                    recipient_account_id: Some(account_id_text(&multisig)),
                    amount_fen: balance_to_i64(amount)?,
                    fee_fen: Some(balance_to_i64(fee)?),
                    block_timestamp: block_ts,
                })
            }
            ("PersonalManage", "PersonalClosed") => {
                let multisig = fields.at("account_id").and_then(extract_account_id)?;
                let beneficiary = fields
                    .at("beneficiary_account_id")
                    .and_then(extract_account_id)?;
                let amount = fields.at("amount").and_then(extract_balance)?;
                let fee = fields.at("fee").and_then(extract_balance)?;
                Some(TxRecordInsert {
                    block_number,
                    extrinsic_index,
                    event_index: 0,
                    tx_type: "institution_multisig_close",
                    sender_account_id: Some(account_id_text(&multisig)),
                    recipient_account_id: Some(account_id_text(&beneficiary)),
                    amount_fen: balance_to_i64(amount)?,
                    fee_fen: Some(balance_to_i64(fee)?),
                    block_timestamp: block_ts,
                })
            }

            // ─── destroy (index 14) ───────────────────────
            ("ResolutionDestroy", "DestroyExecuted") => {
                let amount = fields.at("amount").and_then(extract_balance)?;
                Some(TxRecordInsert {
                    block_number,
                    extrinsic_index,
                    event_index: 0,
                    tx_type: "fund_destroy",
                    sender_account_id: None,
                    recipient_account_id: None,
                    amount_fen: balance_to_i64(amount)?,
                    fee_fen: None,
                    block_timestamp: block_ts,
                })
            }

            _ => None,
        }
    })();
    record.map(Some).ok_or_else(|| {
        format!("invalid required fields in {pallet}::{variant} at block {block_number}")
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "../tests/indexer_event_parser.rs"]
mod tests;

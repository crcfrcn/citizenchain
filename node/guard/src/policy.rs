//! 依据实际事件和metadata核对受保护账户及固定费用政策。

use crate::guard::decode_exact;
#[cfg(test)]
use codec::Decode;
use codec::Encode;
use frame_metadata::RuntimeMetadata;
#[cfg(test)]
use frame_metadata::RuntimeMetadataPrefixed;
#[cfg(test)]
use frame_system::Phase;
#[cfg(test)]
use runtime::{RuntimeCall, UncheckedExtrinsic};
use scale_value::Composite;
use scale_value::Value;
use scale_value::ValueDef;

use sp_runtime::traits::BlakeTwo256;
#[cfg(test)]
use sp_runtime::traits::LazyExtrinsic;
use sp_state_machine::Backend;
use sp_state_machine::OverlayedChanges;
use sp_state_machine::StateMachine;
use std::collections::BTreeMap;

const OFFCHAIN_PALLET: &[u8] = b"OffchainTransaction";

const OFFCHAIN_RATE: &[u8] = b"L2FeeRateBp";

const OFFCHAIN_RATE_PROPOSED: &[u8] = b"L2FeeRateProposed";

const OFFCHAIN_MAX_RATE: &[u8] = b"MaxL2FeeRateBp";

const SYSTEM_EVENTS: &[u8] = b"Events";

/// 链下费率字段内部仍按百分之一百分点计数；10 对应制度上限 0.1%。
const OFFCHAIN_MAX_RATE_UNITS: u32 = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProtectedFee {
    Onchain {
        payer_account_id: [u8; 32],
        amount: u128,
        has_remark: bool,
    },
    Vote {
        payer_account_id: [u8; 32],
    },
}

fn protected_fee(
    call: &Value<()>,
    payer: Option<[u8; 32]>,
    remark_fee_enabled: bool,
) -> Result<Option<ProtectedFee>, String> {
    let (pallet, fields) = variant(call).ok_or("交易缺少 RuntimeCall 类型")?;
    let call = only_field(fields).ok_or("交易 pallet 调用结构非法")?;
    let (name, fields) = variant(call).ok_or("交易缺少 pallet Call 类型")?;
    let transfer = pallet == "OnchainTransaction" && name == "transfer_with_remark";
    let vote = matches!(
        (pallet, name),
        ("InternalVote", "cast")
            | ("JointVote", "cast_admin" | "cast_referendum")
            | (
                "LegislationVote",
                "cast_representative_vote"
                    | "cast_referendum_vote"
                    | "executive_sign"
                    | "override_sign"
                    | "guard_vote"
            )
            | ("ElectionVote", "cast_popular_vote" | "cast_mutual_vote")
    );
    if !transfer && !vote {
        return Ok(None);
    }
    let payer_account_id = payer.ok_or("受保护收费调用缺少可验证的付款账户")?;
    Ok(Some(if transfer {
        ProtectedFee::Onchain {
            payer_account_id,
            amount: composite_field(fields, "amount")
                .and_then(integer)
                .ok_or("链上转账 amount 类型非法")?,
            has_remark: {
                let mut remark = composite_field(fields, "remark").ok_or("链上转账缺少 remark")?;
                // BoundedVec 的类型包装也会解成 Composite；借用解包，避免把空备注误判为非空。
                loop {
                    match &remark.value {
                        ValueDef::Composite(Composite::Named(values)) if values.len() == 1 => {
                            remark = &values[0].1
                        }
                        ValueDef::Composite(Composite::Unnamed(values)) => {
                            if values.len() == 1
                                && matches!(&values[0].value, ValueDef::Composite(_))
                            {
                                remark = &values[0];
                            } else {
                                break remark_fee_enabled && !values.is_empty();
                            }
                        }
                        _ => return Err("链上转账 remark 类型非法".into()),
                    }
                }
            },
        }
    } else {
        ProtectedFee::Vote { payer_account_id }
    }))
}

fn expected_fee(rule: ProtectedFee) -> ([u8; 32], u128) {
    match rule {
        crate::guard::runtime_policy::ProtectedFee::Onchain {
            payer_account_id,
            amount,
            has_remark,
        } => (
            payer_account_id,
            primitives::fee_policy::calculate_onchain_fee(amount).saturating_add(if has_remark {
                primitives::fee_policy::FLAT_FEE
            } else {
                0
            }),
        ),
        crate::guard::runtime_policy::ProtectedFee::Vote { payer_account_id } => {
            (payer_account_id, primitives::fee_policy::FLAT_FEE)
        }
    }
}

/// System::Events 由执行本块的父块 Runtime 编码；不能用节点内置的 RuntimeEvent 解码。
/// Runtime 升级块尤其会包含旧版无字段的 System::CodeUpdated，而新版定义附带 hash。
#[derive(Debug)]
pub(crate) enum FeeEvent {
    Paid {
        extrinsic: Option<u32>,
        account_id: [u8; 32],
        fee: u128,
    },
    Settled {
        recipient_bank_cid: Vec<u8>,
        transfer_amount: u128,
        fee_amount: u128,
    },
}

fn event_storage_type(metadata_raw: &[u8]) -> Result<(scale_info::PortableRegistry, u32), String> {
    let (types, event_type) = match nodeclient::codec::decode_runtime_metadata(metadata_raw)? {
        RuntimeMetadata::V14(meta) => {
            let ty = meta
                .pallets
                .iter()
                .find(|pallet| pallet.name == "System")
                .and_then(|pallet| pallet.storage.as_ref())
                .and_then(|storage| storage.entries.iter().find(|entry| entry.name == "Events"))
                .and_then(|entry| match &entry.ty {
                    frame_metadata::v14::StorageEntryType::Plain(ty) => Some(ty.id),
                    _ => None,
                });
            (meta.types, ty)
        }
        RuntimeMetadata::V15(meta) => {
            let ty = meta
                .pallets
                .iter()
                .find(|pallet| pallet.name == "System")
                .and_then(|pallet| pallet.storage.as_ref())
                .and_then(|storage| storage.entries.iter().find(|entry| entry.name == "Events"))
                .and_then(|entry| match &entry.ty {
                    frame_metadata::v15::StorageEntryType::Plain(ty) => Some(ty.id),
                    _ => None,
                });
            (meta.types, ty)
        }
        RuntimeMetadata::V16(meta) => {
            let ty = meta
                .pallets
                .iter()
                .find(|pallet| pallet.name == "System")
                .and_then(|pallet| pallet.storage.as_ref())
                .and_then(|storage| storage.entries.iter().find(|entry| entry.name == "Events"))
                .and_then(|entry| match &entry.ty {
                    frame_metadata::v16::StorageEntryType::Plain(ty) => Some(ty.id),
                    _ => None,
                });
            (meta.types, ty)
        }
        _ => unreachable!("元数据版本已校验"),
    };
    Ok((
        types,
        event_type.ok_or("父块 Runtime 元数据缺少 System::Events Plain 类型")?,
    ))
}

fn named_field<'a>(value: &'a Value<()>, name: &str) -> Option<&'a Value<()>> {
    match &value.value {
        ValueDef::Composite(fields) => composite_field(fields, name),
        _ => None,
    }
}

fn composite_field<'a>(fields: &'a Composite<()>, name: &str) -> Option<&'a Value<()>> {
    match fields {
        Composite::Named(fields) => fields
            .iter()
            .find_map(|(key, field)| (key == name).then_some(field)),
        Composite::Unnamed(_) => None,
    }
}

fn variant(value: &Value<()>) -> Option<(&str, &Composite<()>)> {
    match &value.value {
        ValueDef::Variant(variant) => Some((&variant.name, &variant.values)),
        _ => None,
    }
}

fn only_field(fields: &Composite<()>) -> Option<&Value<()>> {
    (fields.len() == 1)
        .then(|| fields.values().next())
        .flatten()
}

fn exact_fields(fields: &Composite<()>, expected: &[&str]) -> bool {
    matches!(fields, Composite::Named(values) if values.len() == expected.len()
        && expected.iter().all(|name| values.iter().any(|(field, _)| field == name)))
}

fn integer(value: &Value<()>) -> Option<u128> {
    value.as_u128().or_else(|| match &value.value {
        ValueDef::Composite(fields) => only_field(fields).and_then(integer),
        _ => None,
    })
}

fn bytes(value: &Value<()>) -> Option<Vec<u8>> {
    match &value.value {
        ValueDef::Composite(Composite::Unnamed(values)) => {
            if values.len() == 1 && matches!(&values[0].value, ValueDef::Composite(_)) {
                return bytes(&values[0]);
            }
            values
                .iter()
                .map(|part| u8::try_from(integer(part)?).ok())
                .collect()
        }
        ValueDef::Composite(Composite::Named(values)) if values.len() == 1 => bytes(&values[0].1),
        _ => None,
    }
}

fn decode_events<F>(read_post: &F, metadata_raw: &[u8]) -> Result<Vec<FeeEvent>, String>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let Some(raw) = read_post(&crate::guard::runtime_policy::storage_key::events()) else {
        return Ok(Vec::new());
    };
    let (types, event_type) = event_storage_type(metadata_raw)?;
    let mut input = raw.as_slice();
    let decoded = scale_value::scale::decode_as_type(&mut input, event_type, &types)
        .map_err(|_| "System::Events SCALE 解码失败".to_string())?
        .remove_context();
    if !input.is_empty() {
        return Err("System::Events 存在尾随字节".into());
    }
    let ValueDef::Composite(Composite::Unnamed(records)) = decoded.value else {
        return Err("System::Events 不是事件列表".into());
    };
    let mut fees = Vec::new();
    for record in &records {
        let phase = named_field(record, "phase").ok_or("System::Events 缺少 phase")?;
        let extrinsic = match variant(phase) {
            Some(("ApplyExtrinsic", fields)) => Some(
                only_field(fields)
                    .and_then(integer)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or("System::Events extrinsic phase 格式非法")?,
            ),
            Some(("Initialization" | "Finalization", fields)) if fields.is_empty() => None,
            _ => return Err("System::Events phase 格式非法".into()),
        };
        let event = named_field(record, "event").ok_or("System::Events 缺少 event")?;
        let (pallet, outer_fields) = variant(event).ok_or("System::Events event 格式非法")?;
        if pallet != "OnchainTransaction" && pallet != "OffchainTransaction" {
            continue;
        }
        let inner = only_field(outer_fields).ok_or("System::Events 业务事件格式非法")?;
        let (name, fields) = variant(inner).ok_or("System::Events 业务事件类型非法")?;
        if pallet == "OnchainTransaction" && name == "FeePaid" {
            if !exact_fields(fields, &["account_id", "fee"]) {
                return Err("FeePaid 字段格式非法".into());
            }
            let account = composite_field(fields, "account_id")
                .and_then(bytes)
                .ok_or("FeePaid 账户格式非法")?;
            let account_id: [u8; 32] = account
                .try_into()
                .map_err(|_| "FeePaid 账户长度非法".to_string())?;
            let fee = composite_field(fields, "fee")
                .and_then(integer)
                .ok_or("FeePaid 金额格式非法")?;
            fees.push(crate::guard::runtime_policy::FeeEvent::Paid {
                extrinsic,
                account_id,
                fee,
            });
        } else if pallet == "OffchainTransaction" && name == "PaymentSettled" {
            if !exact_fields(
                fields,
                &[
                    "tx_id",
                    "payer_account_id",
                    "payer_bank_cid",
                    "recipient_account_id",
                    "recipient_bank_cid",
                    "transfer_amount",
                    "fee_amount",
                ],
            ) {
                return Err("PaymentSettled 字段格式非法".into());
            }
            fees.push(crate::guard::runtime_policy::FeeEvent::Settled {
                recipient_bank_cid: composite_field(fields, "recipient_bank_cid")
                    .and_then(bytes)
                    .ok_or("PaymentSettled 收款行 CID 格式非法")?,
                transfer_amount: composite_field(fields, "transfer_amount")
                    .and_then(integer)
                    .ok_or("PaymentSettled 转账金额格式非法")?,
                fee_amount: composite_field(fields, "fee_amount")
                    .and_then(integer)
                    .ok_or("PaymentSettled 手续费格式非法")?,
            });
        }
    }
    Ok(fees)
}

// 唯一历史名称只用于识别不可改写的旧 metadata，不作为当前接口别名。
fn remark_fee_enabled(
    metadata: &RuntimeMetadata,
    parent_enabled: Option<bool>,
) -> Result<bool, String> {
    let constants: Vec<(&str, &[u8])> = match metadata {
        RuntimeMetadata::V14(meta) => meta
            .pallets
            .iter()
            .find(|p| p.name == "OnchainTransaction")
            .map(|p| {
                p.constants
                    .iter()
                    .map(|c| (c.name.as_str(), c.value.as_slice()))
                    .collect()
            }),
        RuntimeMetadata::V15(meta) => meta
            .pallets
            .iter()
            .find(|p| p.name == "OnchainTransaction")
            .map(|p| {
                p.constants
                    .iter()
                    .map(|c| (c.name.as_str(), c.value.as_slice()))
                    .collect()
            }),
        RuntimeMetadata::V16(meta) => meta
            .pallets
            .iter()
            .find(|p| p.name == "OnchainTransaction")
            .map(|p| {
                p.constants
                    .iter()
                    .map(|c| (c.name.as_str(), c.value.as_slice()))
                    .collect()
            }),
        _ => return Err("费用 metadata 版本不受支持".into()),
    }
    .ok_or("metadata 缺少 OnchainTransaction")?;
    let mut policy = None;
    for (name, raw) in constants {
        let enabled = match name {
            "FLAT_FEE" => true,
            "VoteFlatFee" => false,
            _ => continue,
        };
        if policy.is_some()
            || decode_exact::<u128>(raw, "定额费")? != primitives::fee_policy::FLAT_FEE
        {
            return Err("metadata 定额费重复或不等于 FLAT_FEE".into());
        }
        policy = Some(enabled);
    }
    let enabled = policy.ok_or("metadata 缺少定额费")?;
    if parent_enabled == Some(true) && !enabled {
        return Err("新定额费规则生效后禁止退回旧收费名称".into());
    }
    Ok(enabled)
}

/// 交易与事件共用父 Runtime 的真实类型；历史 V4 不能交给现行 V5 包装类型解码。
struct ExtrinsicSchema {
    types: scale_info::PortableRegistry,
    address: u32,
    signature: u32,
    call: u32,
    versions: Vec<u8>,
    extensions: BTreeMap<u8, Vec<(String, u32)>>,
    remark_fee_enabled: bool,
}

impl ExtrinsicSchema {
    fn new(raw: &[u8]) -> Result<Self, String> {
        let metadata = nodeclient::codec::decode_runtime_metadata(raw)?;
        let remark_fee_enabled = remark_fee_enabled(&metadata, None)?;
        let (types, address, signature, call, versions, extensions) =
            match metadata {
                RuntimeMetadata::V14(meta) => {
                    if !matches!(meta.extrinsic.version, 4 | 5) {
                        return Err("父 Runtime 声明的交易格式非法".into());
                    }
                    let xt = meta
                        .types
                        .resolve(meta.extrinsic.ty.id)
                        .ok_or("父 Runtime 缺少 extrinsic 类型")?;
                    let param = |name: &str| {
                        xt.type_params
                            .iter()
                            .find(|param| param.name == name)
                            .and_then(|param| param.ty)
                            .map(|ty| ty.id)
                            .ok_or_else(|| format!("extrinsic 缺少 {name} 类型"))
                    };
                    let (address, signature, call) =
                        (param("Address")?, param("Signature")?, param("Call")?);
                    let extensions = meta
                        .extrinsic
                        .signed_extensions
                        .into_iter()
                        .map(|ext| (ext.identifier, ext.ty.id))
                        .collect();
                    (
                        meta.types,
                        address,
                        signature,
                        call,
                        Vec::new(),
                        BTreeMap::from([(0, extensions)]),
                    )
                }
                RuntimeMetadata::V15(meta) => {
                    if !matches!(meta.extrinsic.version, 4 | 5) {
                        return Err("父 Runtime 声明的交易格式非法".into());
                    }
                    let extensions = meta
                        .extrinsic
                        .signed_extensions
                        .into_iter()
                        .map(|ext| (ext.identifier, ext.ty.id))
                        .collect();
                    (
                        meta.types,
                        meta.extrinsic.address_ty.id,
                        meta.extrinsic.signature_ty.id,
                        meta.extrinsic.call_ty.id,
                        Vec::new(),
                        BTreeMap::from([(0, extensions)]),
                    )
                }
                RuntimeMetadata::V16(meta) => {
                    if meta.extrinsic.versions.is_empty() {
                        return Err("父 Runtime 缺少交易格式声明".into());
                    }
                    let mut extensions = BTreeMap::new();
                    for (version, indexes) in meta.extrinsic.transaction_extensions_by_version {
                        let mut pipeline = Vec::new();
                        for index in indexes {
                            let ext = meta
                                .extrinsic
                                .transaction_extensions
                                .get(index.0 as usize)
                                .ok_or("交易扩展元数据索引越界")?;
                            pipeline.push((ext.identifier.clone(), ext.ty.id));
                        }
                        extensions.insert(version, pipeline);
                    }
                    (
                        meta.types,
                        meta.extrinsic.address_ty.id,
                        meta.extrinsic.signature_ty.id,
                        meta.extrinsic.call_ty.id,
                        meta.extrinsic.versions,
                        extensions,
                    )
                }
                _ => unreachable!("元数据版本已校验"),
            };
        Ok(Self {
            types,
            address,
            signature,
            call,
            versions,
            extensions,
            remark_fee_enabled,
        })
    }

    fn decode(&self, input: &mut &[u8], ty: u32) -> Result<Value<()>, String> {
        scale_value::scale::decode_as_type(input, ty, &self.types)
            .map(Value::remove_context)
            .map_err(|error| format!("父 Runtime 交易类型解码失败:{error}"))
    }

    fn account(value: &Value<()>) -> Result<[u8; 32], String> {
        bytes(value)
            .and_then(|value| value.try_into().ok())
            .ok_or_else(|| "交易付款账户不是完整的32字节账户".into())
    }

    fn proof_account(value: &Value<()>) -> Result<[u8; 32], String> {
        // Authenticate 的透明组合包装只允许逐层唯一字段，不按字节偏移猜测账户。
        let mut value = value;
        while let ValueDef::Composite(fields) = &value.value {
            value = only_field(fields).ok_or("AccountAuth 包装字段非法")?;
        }
        let (name, fields) = variant(value).ok_or("AccountAuth 缺少认证证明")?;
        if !matches!(name, "Sr25519" | "First" | "Stored") {
            return Err("AccountAuth 认证证明类型非法".into());
        }
        Self::account(composite_field(fields, "account").ok_or("认证证明缺少 account")?)
    }

    fn fee(&self, opaque: &sp_runtime::OpaqueExtrinsic) -> Result<Option<ProtectedFee>, String> {
        let (&tag, mut input) = opaque.inner().split_first().ok_or("交易信封为空")?;
        if !matches!(tag, 0x04 | 0x05 | 0x84 | 0x45) {
            return Err("交易信封版本或类型非法".into());
        }
        // V14/V15 只记录单个格式号；V16 的完整版本表按原样约束。
        if !self.versions.is_empty() && !self.versions.contains(&(tag & 0x3f)) {
            return Err("父 Runtime 未声明该交易格式".into());
        }
        let mut payer = None;
        let extension_version = match tag {
            0x84 => {
                let address = self.decode(&mut input, self.address)?;
                let ("Id", fields) = variant(&address).ok_or("签名交易地址类型非法")?
                else {
                    return Err("签名交易必须提供完整账户地址".into());
                };
                payer = Some(Self::account(
                    only_field(fields).ok_or("签名账户字段非法")?,
                )?);
                self.decode(&mut input, self.signature)?;
                Some(0)
            }
            0x45 => {
                let (&version, remaining) = input.split_first().ok_or("交易缺少扩展版本")?;
                input = remaining;
                Some(version)
            }
            _ => None,
        };
        if let Some(version) = extension_version {
            let pipeline = self
                .extensions
                .get(&version)
                .ok_or("父 Runtime 未声明该交易扩展版本")?;
            for (identifier, ty) in pipeline {
                let value = self.decode(&mut input, *ty)?;
                if identifier == <chain_signing::mldsa::Authenticate<runtime::Runtime> as
                    sp_runtime::traits::TransactionExtension<runtime::RuntimeCall>>::IDENTIFIER {
                    if payer.is_some() {
                        return Err("交易重复声明付款认证".into());
                    }
                    payer = Some(Self::proof_account(&value)?);
                }
            }
        }
        let call = self.decode(&mut input, self.call)?;
        if !input.is_empty() {
            return Err("交易包含尾随字节".into());
        }
        protected_fee(&call, payer, self.remark_fee_enabled)
    }
}

fn decode_body(
    body: &[sp_runtime::OpaqueExtrinsic],
    metadata: &[u8],
) -> Result<Vec<Option<ProtectedFee>>, String> {
    let schema = ExtrinsicSchema::new(metadata)?;
    body.iter().map(|opaque| schema.fee(opaque)).collect()
}

fn offchain_fee(amount: u128, rate_units: u32) -> u128 {
    // rate_units 每单位为 0.01%；加 5_000 后按万分比四舍五入，最低 1 分。
    amount
        .saturating_mul(u128::from(rate_units))
        .saturating_add(5_000)
        .checked_div(10_000)
        .unwrap_or_default()
        .max(primitives::fee_policy::OFFCHAIN_MIN_FEE)
}

fn check_rate_value(rate: u32) -> Result<(), String> {
    // 制度只冻结最高费率，不冻结费率下限；0% 仍按每笔最低 1 分收费。
    if rate > OFFCHAIN_MAX_RATE_UNITS {
        return Err(format!("清算行费率 {rate} 超过固定上限 0.1%"));
    }
    Ok(())
}

fn check_max_rate(raw: Option<Vec<u8>>) -> Result<(), String> {
    let Some(raw) = raw else {
        // ValueQuery 的缺省 0 表示采用节点固定上限 0.1%。
        return Ok(());
    };
    let rate: u32 = decode_exact(&raw, "OffchainTransaction::MaxL2FeeRateBp")?;
    if rate != 0 {
        check_rate_value(rate)?;
    }
    Ok(())
}

/// 校验普通区块实际产生的固定链上费用和链下清算费用。
pub(crate) fn check_block<F>(
    body: &[sp_runtime::OpaqueExtrinsic],
    read_post: F,
    parent_metadata: &[u8],
) -> Result<(), String>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let extrinsics = decode_body(body, parent_metadata)?;
    let events = decode_events(&read_post, parent_metadata)?;

    for (index, rule) in extrinsics.into_iter().enumerate() {
        let Some(rule) = rule else {
            continue;
        };
        let ordinary_transfer = matches!(rule, ProtectedFee::Onchain { .. });
        let (expected_payer, expected_amount) = expected_fee(rule);
        let mut paid = events.iter().filter(|record| {
            matches!(record, FeeEvent::Paid { extrinsic: Some(event_index), .. } if *event_index == index as u32)
        });
        let matches_expected = |record: &FeeEvent| {
            matches!(record, FeeEvent::Paid { account_id, fee, .. }
                if *account_id == expected_payer && *fee == expected_amount)
        };
        let found = if ordinary_transfer {
            // 普通转账只能有一次完整扣费；投票仍允许合法业务回调的执行费用。
            paid.next().is_some_and(matches_expected) && paid.next().is_none()
        } else {
            paid.any(matches_expected)
        };
        if !found {
            return Err(format!(
                "第 {index} 笔交易 FeePaid 缺失、金额不符或普通转账重复收费:账户 0x{},金额 {expected_amount} 分",
                hex::encode(expected_payer)
            ));
        }
    }

    for record in &events {
        let crate::guard::runtime_policy::FeeEvent::Settled {
            recipient_bank_cid,
            transfer_amount,
            fee_amount,
        } = record
        else {
            continue;
        };
        let raw = read_post(&crate::guard::runtime_policy::storage_key::rate(
            recipient_bank_cid.as_slice(),
        ))
        .ok_or("链下清算结果缺少收款方清算行费率")?;
        let rate: u32 = decode_exact(&raw, "OffchainTransaction::L2FeeRateBp")?;
        check_rate_value(rate)?;
        let expected = offchain_fee(*transfer_amount, rate);
        if *fee_amount != expected {
            return Err(format!(
                "链下清算手续费错误:期望 {expected} 分,实际 {fee_amount} 分"
            ));
        }
    }
    Ok(())
}

/// 校验本块触及的清算行费率状态；全局上限永久不得超过 0.1%。
pub(crate) fn check_transition<F>(
    delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    read_post: F,
) -> Result<(), String>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    check_max_rate(read_post(
        &crate::guard::runtime_policy::storage_key::max_rate(),
    ))?;
    let rate_prefix = crate::guard::runtime_policy::storage_key::rate_prefix();
    let proposed_prefix = crate::guard::runtime_policy::storage_key::proposed_prefix();
    for key in delta.keys() {
        if key.starts_with(&rate_prefix) {
            if let Some(raw) = read_post(key) {
                check_rate_value(decode_exact(&raw, "OffchainTransaction::L2FeeRateBp")?)?;
            }
        } else if key.starts_with(&proposed_prefix) {
            if let Some(raw) = read_post(key) {
                let (rate, _effective_at): (u32, u32) =
                    decode_exact(&raw, "OffchainTransaction::L2FeeRateProposed")?;
                check_rate_value(rate)?;
            }
        }
    }
    Ok(())
}

/// 校验 runtime 升级时的清算行费率存量。
pub(crate) fn check_imported_state<'a, I>(entries: I) -> Result<(), String>
where
    I: IntoIterator<Item = (&'a Vec<u8>, &'a Vec<u8>)>,
{
    let state = entries.into_iter().collect::<BTreeMap<_, _>>();
    check_max_rate(
        state
            .get(&crate::guard::runtime_policy::storage_key::max_rate())
            .map(|raw| (*raw).clone()),
    )?;
    let rate_prefix = crate::guard::runtime_policy::storage_key::rate_prefix();
    let proposed_prefix = crate::guard::runtime_policy::storage_key::proposed_prefix();
    for (key, raw) in state {
        if key.starts_with(&rate_prefix) {
            check_rate_value(decode_exact(raw, "OffchainTransaction::L2FeeRateBp")?)?;
        } else if key.starts_with(&proposed_prefix) {
            let (rate, _effective_at): (u32, u32) =
                decode_exact(raw, "OffchainTransaction::L2FeeRateProposed")?;
            check_rate_value(rate)?;
        }
    }
    Ok(())
}

fn call_candidate<B>(
    backend: &B,
    overlay: &mut OverlayedChanges<BlakeTwo256>,
    executor: &sc_executor::WasmExecutor<sp_io::SubstrateHostFunctions>,
    runtime_code: &sp_core::traits::RuntimeCode<'_>,
    parent_hash: sp_core::H256,
    method: &str,
    data: &[u8],
) -> Result<Vec<u8>, String>
where
    B: Backend<BlakeTwo256>,
{
    let mut extensions = Default::default();
    let result = StateMachine::new(
        backend,
        overlay,
        executor,
        method,
        data,
        &mut extensions,
        runtime_code,
        sp_core::traits::CallContext::Onchain { import: false },
    )
    .set_parent_hash(parent_hash)
    .execute()
    .map_err(|error| format!("候选 runtime 调用 {method} 失败:{error}"));
    result
}

/// 只检查候选 runtime 的接口策略，不执行合成业务交易。
pub(crate) fn check_candidate_runtime<B>(
    backend: &B,
    parent_hash: sp_core::H256,
    post_delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    candidate_code: &[u8],
    parent_metadata: Option<&[u8]>,
) -> Result<(), String>
where
    B: Backend<BlakeTwo256>,
{
    use std::borrow::Cow;
    let mut overlay = OverlayedChanges::<BlakeTwo256>::default();
    for (key, value) in post_delta {
        overlay.set_storage(key.clone(), value.clone());
    }
    let fetcher = sp_core::traits::WrappedRuntimeCode(Cow::Borrowed(candidate_code));
    let runtime_code = sp_core::traits::RuntimeCode {
        code_fetcher: &fetcher,
        heap_pages: None,
        hash: sp_crypto_hashing::blake2_256(candidate_code).to_vec(),
    };
    let executor = sc_executor::WasmExecutor::<sp_io::SubstrateHostFunctions>::default();

    let version_raw = call_candidate(
        backend,
        &mut overlay,
        &executor,
        &runtime_code,
        parent_hash,
        "Core_version",
        &[],
    )?;
    let version: sc_executor::RuntimeVersion = decode_exact(&version_raw, "候选 RuntimeVersion")?;
    let benchmark_api_id =
        <dyn frame_benchmarking::Benchmark<runtime::Block> as sp_api::RuntimeApiInfo>::ID;
    if version
        .apis
        .iter()
        .any(|(api_id, _)| api_id == &benchmark_api_id)
    {
        return Err("候选 runtime 暴露 Benchmark runtime API，禁止作为正式 :code".to_string());
    }

    let metadata_raw = call_candidate(
        backend,
        &mut overlay,
        &executor,
        &runtime_code,
        parent_hash,
        "Metadata_metadata",
        &[],
    )?;
    let metadata: sp_core::OpaqueMetadata = decode_exact(&metadata_raw, "候选 metadata")?;
    let parent_enabled = parent_metadata
        .map(|raw| remark_fee_enabled(&nodeclient::codec::decode_runtime_metadata(raw)?, None))
        .transpose()?;
    remark_fee_enabled(
        &nodeclient::codec::decode_runtime_metadata(&metadata)?,
        parent_enabled,
    )?;

    // 节点二进制级死规则:个人多签禁强制公民 CID。候选 runtime 必须暴露 `AdminPolicyApi`
    // 且 `personal_multisig_cid_mandated() == false`;返回 true(强制)或缺失该 API(下方
    // `?` 传播 Err)一律判 KnownBad,防 runtime 升级把个人多签 CID 改成强制或移除守卫。
    let cid_mandated_raw = call_candidate(
        backend,
        &mut overlay,
        &executor,
        &runtime_code,
        parent_hash,
        "AdminPolicyApi_personal_multisig_cid_mandated",
        &[],
    )?;
    let cid_mandated: bool = decode_exact(
        &cid_mandated_raw,
        "候选 AdminPolicyApi 个人多签 CID 强制策略",
    )?;
    if cid_mandated {
        return Err("候选 runtime 违规强制个人多签提供公民 CID(禁强制死规则)".to_string());
    }

    Ok(())
}

pub mod storage_key {
    use super::*;
    use crate::guard::runtime_policy::OFFCHAIN_MAX_RATE;
    use crate::guard::runtime_policy::OFFCHAIN_PALLET;
    use crate::guard::runtime_policy::OFFCHAIN_RATE;
    use crate::guard::runtime_policy::OFFCHAIN_RATE_PROPOSED;
    use crate::guard::runtime_policy::SYSTEM_EVENTS;
    use sp_crypto_hashing::blake2_128;
    use sp_crypto_hashing::twox_128;

    fn prefix(item: &[u8]) -> Vec<u8> {
        [twox_128(OFFCHAIN_PALLET), twox_128(item)].concat()
    }

    pub fn events() -> Vec<u8> {
        [twox_128(b"System"), twox_128(SYSTEM_EVENTS)].concat()
    }

    pub fn rate_prefix() -> Vec<u8> {
        prefix(OFFCHAIN_RATE)
    }

    pub fn proposed_prefix() -> Vec<u8> {
        prefix(OFFCHAIN_RATE_PROPOSED)
    }

    pub fn max_rate() -> Vec<u8> {
        prefix(OFFCHAIN_MAX_RATE)
    }

    pub fn rate(bank_cid: &[u8]) -> Vec<u8> {
        // CID 键 = InstitutionCidNumber(BoundedVec<u8>),SCALE = Compact(len) || bytes。
        let encoded = bank_cid.to_vec().encode();
        [rate_prefix(), blake2_128(&encoded).to_vec(), encoded].concat()
    }

    pub fn relevant_prefixes() -> [Vec<u8>; 2] {
        [rate_prefix(), proposed_prefix()]
    }

    pub fn is_relevant(key: &[u8]) -> bool {
        key == max_rate()
            || relevant_prefixes()
                .iter()
                .any(|prefix| key.starts_with(prefix))
    }
}
#[cfg(test)]
#[path = "../tests/policy.rs"]
mod tests;

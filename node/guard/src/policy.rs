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

const OFFCHAIN_PALLET: &[u8] = b"Offchain";

const OFFCHAIN_RATE: &[u8] = b"L2FeeRateBp";

const OFFCHAIN_RATE_PROPOSED: &[u8] = b"L2FeeRateProposed";

const OFFCHAIN_MAX_RATE: &[u8] = b"MaxL2FeeRateBp";

const SYSTEM_EVENTS: &[u8] = b"Events";

/// 链下费率字段内部仍按百分之一百分点计数；10 对应制度上限 0.1%。
const OFFCHAIN_MAX_RATE_UNITS: u32 = 10;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ProtectedFee {
    Transfers { payer: [u8; 32], mode: String, items: Vec<TransferSpec> },
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
    execution_fee: bool,
) -> Result<Option<ProtectedFee>, String> {
    if execution_fee {
        if let Some(rule) = transfer_rule(call, payer)? { return Ok(Some(rule)); }
    }
    let (pallet, fields) = variant(call).ok_or("交易缺少 RuntimeCall 类型")?;
    let call = only_field(fields).ok_or("交易 pallet 调用结构非法")?;
    let (name, fields) = variant(call).ok_or("交易缺少 pallet Call 类型")?;
    if pallet == "Utility" {return Err("Utility缺少执行期收费规则".into());}
    let transfer =
        matches!(pallet, "Onchain" | "OnchainTransaction") && name == "transfer_with_remark";
    let vote = matches!(
        (pallet, name),
        ("InternalVote", "cast")
            | (
                "JointVote",
                "cast_role_vote" | "cast_admin" | "cast_referendum"
            )
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
        ProtectedFee::Transfers { .. } => unreachable!("执行期转账由逐项事务结果核对"),
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
    let mut fees = Vec::new();
    for (extrinsic,pallet,name,fields) in records(&raw,metadata_raw)? {
        let pallet=pallet.as_str();let name=name.as_str();let fields=&fields;
        if matches!(pallet, "Onchain" | "OnchainTransaction") && name == "FeePaid" {
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
        } else if matches!(pallet, "Offchain" | "OffchainTransaction") && name == "PaymentSettled" {
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

// 旧名称仅由父块或历史metadata解释；当前存储入口固定使用现行注册名。
fn remark_fee_enabled(
    metadata: &RuntimeMetadata,
    parent_enabled: Option<bool>,
) -> Result<bool, String> {
    let pallets: Vec<Vec<(&str, &[u8])>> = match metadata {
        RuntimeMetadata::V14(meta) => meta
            .pallets
            .iter()
            .filter(|p| matches!(p.name.as_str(), "Onchain" | "OnchainTransaction"))
            .map(|p| {
                p.constants
                    .iter()
                    .map(|c| (c.name.as_str(), c.value.as_slice()))
                    .collect()
            })
            .collect(),
        RuntimeMetadata::V15(meta) => meta
            .pallets
            .iter()
            .filter(|p| matches!(p.name.as_str(), "Onchain" | "OnchainTransaction"))
            .map(|p| {
                p.constants
                    .iter()
                    .map(|c| (c.name.as_str(), c.value.as_slice()))
                    .collect()
            })
            .collect(),
        RuntimeMetadata::V16(meta) => meta
            .pallets
            .iter()
            .filter(|p| matches!(p.name.as_str(), "Onchain" | "OnchainTransaction"))
            .map(|p| {
                p.constants
                    .iter()
                    .map(|c| (c.name.as_str(), c.value.as_slice()))
                    .collect()
            })
            .collect(),
        _ => return Err("费用 metadata 版本不受支持".into()),
    };
    let [constants] = pallets.as_slice() else {
        return Err("metadata 链上费用模块缺失或重复".into());
    };
    let mut policy = None;
    for &(name, raw) in constants {
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
    execution_fee: bool,
}

impl ExtrinsicSchema {
    fn new(raw: &[u8]) -> Result<Self, String> {
        let metadata = nodeclient::codec::decode_runtime_metadata(raw)?;
        let remark_fee_enabled = remark_fee_enabled(&metadata, None)?;
        let execution_fee = execution_fee_enabled(&metadata)?;
        let (types, address, signature, call, versions, extensions) = match metadata {
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
            execution_fee,
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
        protected_fee(&call, payer, self.remark_fee_enabled, self.execution_fee)
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
    let rate: u32 = decode_exact(&raw, "Offchain::MaxL2FeeRateBp")?;
    if rate != 0 {
        check_rate_value(rate)?;
    }
    Ok(())
}

fn settlement_rate_key(metadata: &[u8], cid: &[u8]) -> Result<Vec<u8>, String> {
    let metadata = nodeclient::codec::decode_runtime_metadata(metadata)?;
    let pallets: Vec<Option<String>> = match metadata {
        RuntimeMetadata::V14(m) => m
            .pallets
            .into_iter()
            .filter(|p| matches!(p.name.as_str(), "Offchain" | "OffchainTransaction"))
            .map(|p| p.storage.map(|s| s.prefix))
            .collect(),
        RuntimeMetadata::V15(m) => m
            .pallets
            .into_iter()
            .filter(|p| matches!(p.name.as_str(), "Offchain" | "OffchainTransaction"))
            .map(|p| p.storage.map(|s| s.prefix))
            .collect(),
        RuntimeMetadata::V16(m) => m
            .pallets
            .into_iter()
            .filter(|p| matches!(p.name.as_str(), "Offchain" | "OffchainTransaction"))
            .map(|p| p.storage.map(|s| s.prefix))
            .collect(),
        _ => return Err("费用metadata版本不受支持".into()),
    };
    let [Some(prefix)] = pallets.as_slice() else {
        return Err("metadata清算费用模块或存储缺失或重复".into());
    };
    Ok(storage_key::rate_in(prefix.as_bytes(), cid))
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
        if let ProtectedFee::Transfers { payer, mode, items } = &rule {
            let raw = read_post(&storage_key::events()).ok_or("缺少转账结果事件")?;
            inspect_transfers(*payer, mode, items, &records(&raw, parent_metadata)?, index as u32)?;
            continue;
        }
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
        let raw = read_post(&settlement_rate_key(
            parent_metadata,
            recipient_bank_cid.as_slice(),
        )?)
        .ok_or("链下清算结果缺少收款方清算行费率")?;
        let rate: u32 = decode_exact(&raw, "Offchain::L2FeeRateBp")?;
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
    // 历史前缀仅用于校验既存原始状态；不承接当前业务读写或提供旧名回退。
    for pallet in [OFFCHAIN_PALLET, b"OffchainTransaction".as_slice()] {
        check_max_rate(read_post(&storage_key::prefix_in(
            pallet,
            OFFCHAIN_MAX_RATE,
        )))?;
        let rate_prefix = storage_key::prefix_in(pallet, OFFCHAIN_RATE);
        let proposed_prefix = storage_key::prefix_in(pallet, OFFCHAIN_RATE_PROPOSED);
        for key in delta.keys() {
            if key.starts_with(&rate_prefix) {
                if let Some(raw) = read_post(key) {
                    check_rate_value(decode_exact(&raw, "清算行费率")?)?;
                }
            } else if key.starts_with(&proposed_prefix) {
                if let Some(raw) = read_post(key) {
                    let (rate, _effective_at): (u32, u32) = decode_exact(&raw, "清算行待生效费率")?;
                    check_rate_value(rate)?;
                }
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
    // 包括旧区块保留的原始键；每个实际值均校验，不以一个前缀替代另一个。
    for pallet in [OFFCHAIN_PALLET, b"OffchainTransaction".as_slice()] {
        check_max_rate(
            state
                .get(&storage_key::prefix_in(pallet, OFFCHAIN_MAX_RATE))
                .map(|raw| (*raw).clone()),
        )?;
        let rate_prefix = storage_key::prefix_in(pallet, OFFCHAIN_RATE);
        let proposed_prefix = storage_key::prefix_in(pallet, OFFCHAIN_RATE_PROPOSED);
        for (key, raw) in &state {
            if key.starts_with(&rate_prefix) {
                check_rate_value(decode_exact(raw, "清算行费率")?)?;
            } else if key.starts_with(&proposed_prefix) {
                let (rate, _effective_at): (u32, u32) = decode_exact(raw, "清算行待生效费率")?;
                check_rate_value(rate)?;
            }
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
    let parent_execution = parent_metadata.map(|raw| execution_fee_enabled(&nodeclient::codec::decode_runtime_metadata(raw)?)).transpose()?.unwrap_or(false);
    let candidate_execution=execution_fee_enabled(&nodeclient::codec::decode_runtime_metadata(&metadata)?)?;
    if parent_execution && !candidate_execution {
        return Err("普通转账成功收费规则生效后禁止退回外层预扣".into());
    }
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
        prefix_in(OFFCHAIN_PALLET, item)
    }

    pub(super) fn prefix_in(pallet: &[u8], item: &[u8]) -> Vec<u8> {
        [twox_128(pallet), twox_128(item)].concat()
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
        rate_in(OFFCHAIN_PALLET, bank_cid)
    }

    pub(super) fn rate_in(pallet: &[u8], bank_cid: &[u8]) -> Vec<u8> {
        // CID键使用所属块storage前缀及原BoundedVec SCALE。
        let encoded = bank_cid.to_vec().encode();
        [
            prefix_in(pallet, OFFCHAIN_RATE),
            blake2_128(&encoded).to_vec(),
            encoded,
        ]
        .concat()
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


/// 由父metadata解释收费时点；缺失仅对应部署前的历史规则。
fn execution_fee_enabled(metadata: &RuntimeMetadata) -> Result<bool, String> {
    macro_rules! find { ($m:expr) => { $m.pallets.iter().filter(|p| matches!(p.name.as_str(), "Onchain" | "OnchainTransaction")).flat_map(|p| p.constants.iter()).filter(|c| c.name == "TransferFeeTiming").map(|c| c.value.as_slice()).collect::<Vec<_>>() }; }
    let values = match metadata { RuntimeMetadata::V14(m) => find!(m), RuntimeMetadata::V15(m) => find!(m), RuntimeMetadata::V16(m) => find!(m), _ => return Err("费用metadata版本非法".into()) };
    match values.as_slice() {
        [] => Ok(false),
        [raw] if decode_exact::<primitives::fee_policy::FeeTiming>(raw, "转账收费时点")? == primitives::fee_policy::FeeTiming::Execution => Ok(true),
        _ => Err("普通转账收费时点重复或非法".into()),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TransferSpec { to: [u8; 32], amount: u128, remark: Vec<u8> }

fn call_parts(call: &Value<()>) -> Result<(&str, &str, &Composite<()>), String> {
    let (pallet, fields) = variant(call).ok_or("调用类型非法")?;
    let (name, fields) = variant(only_field(fields).ok_or("调用包装非法")?).ok_or("调用类型非法")?;
    Ok((pallet, name, fields))
}
fn spec(fields: &Composite<()>) -> Result<TransferSpec, String> {
    Ok(TransferSpec {
        to: bytes(composite_field(fields, "beneficiary_account_id").ok_or("转账缺少收款人")?).and_then(|v| v.try_into().ok()).ok_or("收款账户非法")?,
        amount: integer(composite_field(fields, "amount").ok_or("转账缺少金额")?).ok_or("金额非法")?,
        remark: bytes(composite_field(fields, "remark").ok_or("转账缺少备注")?).ok_or("备注非法")?,
    })
}
fn transfer_rule(call: &Value<()>, payer: Option<[u8; 32]>) -> Result<Option<ProtectedFee>, String> {
    let (pallet, name, fields) = call_parts(call)?;
    if pallet == "Onchain" && name == "transfer_with_remark" {
        return Ok(Some(ProtectedFee::Transfers { payer: payer.ok_or("缺少签名付款人")?, mode: "transfer".into(), items: vec![spec(fields)?] }));
    }
    if pallet != "Utility" { return Ok(None); }
    if !matches!(name, "batch" | "batch_all" | "force_batch") { return Err("禁止其它Utility入口".into()); }
    let calls = composite_field(fields, "calls").ok_or("缺少批次清单")?;
    let ValueDef::Composite(Composite::Unnamed(calls)) = &calls.value else { return Err("批次清单类型非法".into()); };
    if calls.is_empty() || calls.len() > runtime::configs::batch_calls_limit() as usize { return Err("批次数量非法".into()); }
    let items = calls.iter().map(|call| {
        let (p, n, f) = call_parts(call)?;
        if p != "Onchain" || n != "transfer_with_remark" { return Err("批次包含非普通转账或嵌套".into()); }
        spec(f)
    }).collect::<Result<Vec<_>, String>>()?;
    Ok(Some(ProtectedFee::Transfers { payer: payer.ok_or("批次缺少签名付款人")?, mode: name.into(), items }))
}

type Record = (Option<u32>, String, String, Composite<()>);
fn records(raw: &[u8], metadata: &[u8]) -> Result<Vec<Record>, String> {
    let (types, ty) = event_storage_type(metadata)?;
    let mut input = raw;
    let value = scale_value::scale::decode_as_type(&mut input, ty, &types).map_err(|e| format!("父Runtime事件解码失败: {e}"))?.remove_context();
    if !input.is_empty() { return Err("事件尾随字节".into()); }
    let ValueDef::Composite(Composite::Unnamed(values)) = value.value else { return Err("事件列表非法".into()); };
    values.iter().map(|v| {
        let phase = named_field(v, "phase").and_then(variant).ok_or("事件phase非法")?;
        let index = match phase { ("ApplyExtrinsic", f) => Some(only_field(f).and_then(integer).and_then(|n| u32::try_from(n).ok()).ok_or("事件索引非法")?), ("Initialization" | "Finalization", f) if f.is_empty() => None, _ => return Err("事件phase非法".into()) };
        let (p, f) = named_field(v, "event").and_then(variant).ok_or("事件类型非法")?;
        let (n, f) = only_field(f).and_then(variant).ok_or("事件包装非法")?;
        Ok((index, p.into(), n.into(), f.clone()))
    }).collect()
}

/// 维护备注和转账查询共用按执行父metadata解码的成功判定。
pub fn extrinsic_succeeded(raw: &[u8], metadata: &[u8], index: u32) -> Result<(), String> {
    if result_flag(&records(raw,metadata)?,index)? {Ok(())} else {Err("交易执行失败".into())}
}
fn result_flag(values:&[Record],index:u32)->Result<bool,String>{
    let result=values.iter().filter(|(i,p,n,_)|*i==Some(index)&&p=="System"&&matches!(n.as_str(),"ExtrinsicSuccess"|"ExtrinsicFailed")).map(|(_,_,n,_)|n.as_str()).collect::<Vec<_>>();
    match result.as_slice(){["ExtrinsicSuccess"]=>Ok(true),["ExtrinsicFailed"]=>Ok(false),_=>Err("交易没有唯一执行结果".into())}
}

/// 只读结果，不持有Credit，不做事后退款。
pub struct TransferOutcome { pub state: &'static str, pub fees: Vec<u128>, pub items: Vec<&'static str>, pub error: Option<String> }

fn inspect_transfers(payer: [u8; 32], mode: &str, items: &[TransferSpec], values: &[Record], index: u32) -> Result<TransferOutcome, String> {
    let events = values.iter().filter(|e| e.0 == Some(index)).collect::<Vec<_>>();
    let results = events.iter().filter(|e| e.1 == "System" && matches!(e.2.as_str(), "ExtrinsicSuccess" | "ExtrinsicFailed")).collect::<Vec<_>>();
    if results.len() != 1 { return Err("缺少唯一外层结果".into()); }
    let failed = results[0].2 == "ExtrinsicFailed";
    let burn_count=events.iter().filter(|e| e.1=="Onchain" && e.2=="FeeShareBurnt").count();
    let mut segment=(0usize,0usize,0usize);
    let mut paid = Vec::new(); let mut principals = Vec::new(); let mut status = Vec::new(); let mut terminal = Vec::new();
    for e in &events {
        let f = &e.3;
        match (e.1.as_str(), e.2.as_str()) {
            ("Onchain", "FeeShareBurnt") => {segment.2+=1;},
            ("Onchain", "FeePaid") => {
                if !exact_fields(f,&["account_id","fee"]){return Err("手续费事件字段非法".into());}
                segment.0+=1;
                let account = composite_field(f,"account_id").and_then(bytes).ok_or("费用账户非法")?;
                if account.as_slice() != payer { return Err("错误的手续费付款人".into()); }
                paid.push(composite_field(f,"fee").and_then(integer).ok_or("费用金额非法")?);
            },
            ("Onchain", "TransferWithRemark") => {
                if !exact_fields(f,&["from_account_id","beneficiary_account_id","amount","remark"]){return Err("本金事件字段非法".into());}
                segment.1+=1;
                if composite_field(f,"from_account_id").and_then(bytes).as_deref() != Some(payer.as_slice()) { return Err("本金付款人错误".into()); }
                principals.push(spec(f)?);
            },
            ("Utility", "ItemCompleted") => {if !f.is_empty()||segment.0!=1||segment.1!=1 {return Err("成功项缺少唯一的本金或费用事件".into());} segment=(0,0,0);status.push("success");},
            ("Utility", "ItemFailed") => {if !exact_fields(f,&["error"])||segment!=(0,0,0){return Err("失败项仍留下本金、收费或分账".into());}status.push("failed");},
            ("Utility", "BatchInterrupted") => {
                if !exact_fields(f,&["index","error"])||segment!=(0,0,0){return Err("中断项仍留下本金、收费或分账".into());}
                if composite_field(f,"index").and_then(integer) != Some(status.len() as u128) { return Err("批次中断索引错误".into()); }
                status.push("failed"); terminal.push("BatchInterrupted");
            },
            ("Utility", "BatchCompleted" | "BatchCompletedWithErrors") => terminal.push(e.2.as_str()),
            _ => {},
        }
    }
    if failed {
        if burn_count!=0 || !paid.is_empty() || !principals.is_empty() || !status.is_empty() || !terminal.is_empty() { return Err("失败或整组回滚仍留下本金或收费".into()); }
        return Ok(TransferOutcome { state: if mode == "batch_all" { "rolled_back" } else { "failed" }, fees: vec![0;items.len()], items: vec![if mode == "batch_all" { "rolled_back" } else { "failed" };items.len()], error: Some(format!("{:?}",results[0].3)) });
    }
    if mode!="transfer"&&segment!=(0,0,0){return Err("批次终态后残留未归属业务事件".into());}
    if mode == "transfer" {
        if !status.is_empty() || !terminal.is_empty() { return Err("单笔包含批次事件".into()); } status.push("success");
    } else {
        if mode == "batch" && terminal == ["BatchInterrupted"] { status.resize(items.len(), "not_executed"); }
        else if status.len() != items.len() || terminal.len() != 1 || terminal[0] != (if status.contains(&"failed") { "BatchCompletedWithErrors" } else { "BatchCompleted" }) { return Err("批次终态与逐项结果不符".into()); }
        if mode == "batch_all" && status.iter().any(|s| *s != "success") { return Err("原子批次出现部分成功".into()); }
        if mode == "batch" && terminal != ["BatchInterrupted"] && status.contains(&"failed") { return Err("batch失败后仍继续".into()); }
    }
    if status.len() != items.len() { return Err("子项数量不符".into()); }
    let expected = items.iter().zip(&status).filter(|(_,s)| **s == "success").map(|(i,_)| i.clone()).collect::<Vec<_>>();
    let fees = expected.iter().map(|i| primitives::fee_policy::calculate_transfer_fee(i.amount,!i.remark.is_empty())).collect::<Vec<_>>();
    if (fees.is_empty() && burn_count!=0) || principals != expected || paid != fees { return Err("成功项本金或逐笔手续费缺失、重复或不符".into()); }
    let all_fees = items.iter().zip(&status).map(|(i,s)| if *s == "success" { primitives::fee_policy::calculate_transfer_fee(i.amount,!i.remark.is_empty()) } else {0}).collect();
    Ok(TransferOutcome { state: if status.iter().all(|s| *s == "success") {"success"} else if status.contains(&"success") {"partial"} else {"failed"}, fees: all_fees, items: status, error: None })
}

/// 本地只读RPC复用与永久守卫相同的解析和金额校验。
pub fn transfer_outcome(opaque: &sp_runtime::OpaqueExtrinsic, raw: &[u8], metadata: &[u8], index: u32) -> Result<TransferOutcome, String> {
    let schema = ExtrinsicSchema::new(metadata)?;
    let values = records(raw,metadata)?;
    match schema.fee(opaque)?.ok_or("不是普通转账")? {
        ProtectedFee::Transfers { payer, mode, items } => inspect_transfers(payer,&mode,&items,&values,index),
        rule @ ProtectedFee::Onchain { .. } => {
            let (payer,fee) = expected_fee(rule);
            let paid = decode_events(&|key| (key == storage_key::events()).then(||raw.to_vec()),metadata)?;
            let matching = paid.iter().filter(|e| matches!(e,FeeEvent::Paid{extrinsic:Some(i),..} if *i == index)).collect::<Vec<_>>();
            if matching.len() != 1 || !matches!(matching[0],FeeEvent::Paid{account_id,fee:f,..} if *account_id==payer && *f==fee) { return Err("历史单笔收费不符".into()); }
            let success=result_flag(&values,index)?;
            Ok(TransferOutcome { state:if success {"success"} else {"failed"},fees:vec![fee],items:vec![if success {"success"} else {"failed"}],error:None })
        },
        _ => Err("不是普通转账".into()),
    }
}

/// UI按真实父Runtime能力启用批量入口，部署前普通单笔继续遵守旧费用规则。
pub fn transfer_policy(raw: &[u8]) -> Result<(bool,bool),String> {
    let metadata=nodeclient::codec::decode_runtime_metadata(raw)?;
    let execution=execution_fee_enabled(&metadata)?;
    let utility=match &metadata {RuntimeMetadata::V14(m)=>m.pallets.iter().any(|p|p.name=="Utility"&&p.index==27),RuntimeMetadata::V15(m)=>m.pallets.iter().any(|p|p.name=="Utility"&&p.index==27),RuntimeMetadata::V16(m)=>m.pallets.iter().any(|p|p.name=="Utility"&&p.index==27),_=>false};
    Ok((execution,execution&&utility))
}

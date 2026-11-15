use crate::guard::decode_exact;
#[cfg(test)]
use codec::Decode;
use codec::Encode;
use frame_metadata::RuntimeMetadata;
#[cfg(test)]
use frame_metadata::RuntimeMetadataPrefixed;
#[cfg(test)]
use frame_system::Phase;
use runtime::RuntimeCall;
use runtime::UncheckedExtrinsic;
use scale_value::Composite;
use scale_value::Value;
use scale_value::ValueDef;

use sp_runtime::generic::Preamble;
use sp_runtime::traits::BlakeTwo256;
use sp_runtime::traits::ExtrinsicCall;
use sp_runtime::traits::LazyExtrinsic;
use sp_runtime::MultiAddress;
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
    },
    Vote {
        payer_account_id: [u8; 32],
    },
}

fn signed_account(xt: &UncheckedExtrinsic) -> Option<[u8; 32]> {
    match &xt.0.preamble {
        // 旧高度区块按其父 Runtime 执行；此处仅从已执行区块提取实际收费账户。
        Preamble::Signed(MultiAddress::Id(account), _, _) => Some(account.clone().into()),
        Preamble::General(sp_runtime::traits::ExtensionVariant::Other(pipeline)) => {
            match &pipeline.extension.0 .0 {
                chain_signing::mldsa::Proof::Sr25519 { account, .. }
                | chain_signing::mldsa::Proof::First { account, .. }
                | chain_signing::mldsa::Proof::Stored { account, .. } => Some(*account),
            }
        }
        _ => None,
    }
}

fn protected_fee(xt: &UncheckedExtrinsic) -> Option<ProtectedFee> {
    let payer_account_id = signed_account(xt)?;
    match xt.call() {
        RuntimeCall::OnchainTransaction(onchain::pallet::Call::transfer_with_remark {
            amount,
            ..
        }) => Some(crate::guard::runtime_policy::ProtectedFee::Onchain {
            payer_account_id,
            amount: *amount,
        }),
        RuntimeCall::InternalVote(internalvote::pallet::Call::cast { .. })
        | RuntimeCall::JointVote(jointvote::pallet::Call::cast_admin { .. })
        | RuntimeCall::JointVote(jointvote::pallet::Call::cast_referendum { .. })
        | RuntimeCall::LegislationVote(
            legislationvote::pallet::Call::cast_representative_vote { .. }
            | legislationvote::pallet::Call::cast_referendum_vote { .. }
            | legislationvote::pallet::Call::executive_sign { .. }
            | legislationvote::pallet::Call::override_sign { .. }
            | legislationvote::pallet::Call::guard_vote { .. },
        )
        | RuntimeCall::ElectionVote(
            electionvote::pallet::Call::cast_popular_vote { .. }
            | electionvote::pallet::Call::cast_mutual_vote { .. },
        ) => Some(crate::guard::runtime_policy::ProtectedFee::Vote { payer_account_id }),
        _ => None,
    }
}

fn expected_fee(rule: ProtectedFee) -> ([u8; 32], u128) {
    match rule {
        crate::guard::runtime_policy::ProtectedFee::Onchain {
            payer_account_id,
            amount,
        } => (
            payer_account_id,
            primitives::fee_policy::calculate_onchain_fee(amount),
        ),
        crate::guard::runtime_policy::ProtectedFee::Vote { payer_account_id } => {
            (payer_account_id, primitives::fee_policy::VOTE_FLAT_FEE)
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

fn decode_body(body: &[sp_runtime::OpaqueExtrinsic]) -> Result<Vec<UncheckedExtrinsic>, String> {
    body.iter()
        .map(|opaque| {
            UncheckedExtrinsic::decode_unprefixed(opaque.inner())
                .map_err(|_| "区块 extrinsic 无法按节点固定 runtime 类型解码".to_string())
        })
        .collect()
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
    let extrinsics = decode_body(body)?;
    let events = decode_events(&read_post, parent_metadata)?;

    for (index, xt) in extrinsics.iter().enumerate() {
        let Some(rule) = protected_fee(xt) else {
            continue;
        };
        let (expected_payer, expected_amount) = expected_fee(rule);
        let found = events.iter().any(|record| {
            matches!(record, crate::guard::runtime_policy::FeeEvent::Paid { extrinsic: Some(event_index), account_id, fee }
                if *event_index == index as u32
                    && *account_id == expected_payer
                    && *fee == expected_amount)
        });
        if !found {
            return Err(format!(
                "第 {index} 笔交易缺少节点固定口径 FeePaid:账户 0x{},金额 {expected_amount} 分",
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

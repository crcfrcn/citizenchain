//! 节点侧手续费制度守卫。
//!
//! 本文件不信任 runtime 自报的常量：普通区块按节点固定口径核对实际收费事件与
//! 链下清算结果；遇到 `:code` 变化时，再用候选 WASM 在隔离 overlay 中真实执行
//! 两档链上转账和一次投票，直接核对付款账户余额差额。链下入口开放时还会真实
//! 结算一笔最低手续费交易；当前入口若被 `BaseCallFilter` 明确禁用，则不存在可执行
//! 的链下收费路径。候选代码只要违反固定费用制度，就由外层 `BlockImport` 返回
//! `KnownBad`。

use std::collections::BTreeMap;

use codec::Encode;
use frame_system::{EventRecord, Phase};
use sp_core::{sr25519, Pair as _};
use sp_crypto_hashing::twox_128;
use sp_runtime::{
    generic::Preamble,
    traits::{BlakeTwo256, Header as _, LazyExtrinsic},
    AccountId32, MultiAddress,
};
use sp_state_machine::{Backend, OverlayedChanges, StateMachine};

use citizenchain::{RuntimeCall, RuntimeEvent, UncheckedExtrinsic};

use super::{decode_exact, fullnode_issuance, MAccountData, MAccountInfo};

const OFFCHAIN_PALLET: &[u8] = b"OffchainTransaction";
const OFFCHAIN_RATE: &[u8] = b"L2FeeRateBp";
const OFFCHAIN_RATE_PROPOSED: &[u8] = b"L2FeeRateProposed";
const OFFCHAIN_MAX_RATE: &[u8] = b"MaxL2FeeRateBp";
const SYSTEM_EVENTS: &[u8] = b"Events";

/// 链下费率字段内部仍按百分之一百分点计数；10 对应制度上限 0.1%。
const OFFCHAIN_MAX_RATE_UNITS: u32 = 10;
const SYNTHETIC_BALANCE: u128 = 1_000_000_000_000;
// 公民身份调用的最大 proof-size 交易费高于普通手续费探针，使用隔离 overlay 的
// 大额合成余额，确保测试一定进入 pallet 验签分支而不是提前止于 Payment。
const CITIZEN_SIGNATURE_PROBE_BALANCE: u128 = u128::MAX / 4;
const BALANCES_NEW_ACCOUNT_FLAGS: u128 = 0x80000000_00000000_00000000_00000000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProtectedFee {
    Onchain {
        payer_account_id: [u8; 32],
        amount: u128,
    },
    Vote {
        payer_account_id: [u8; 32],
    },
}

pub mod storage_key {
    use super::*;
    use sp_crypto_hashing::blake2_128;

    fn prefix(item: &[u8]) -> Vec<u8> {
        [twox_128(OFFCHAIN_PALLET), twox_128(item)].concat()
    }

    fn pallet_prefix(pallet: &[u8], item: &[u8]) -> Vec<u8> {
        [twox_128(pallet), twox_128(item)].concat()
    }

    fn map_key(pallet: &[u8], item: &[u8], encoded_key: &[u8]) -> Vec<u8> {
        [
            pallet_prefix(pallet, item),
            blake2_128(encoded_key).to_vec(),
            encoded_key.to_vec(),
        ]
        .concat()
    }

    fn double_map_key(pallet: &[u8], item: &[u8], first: &[u8], second: &[u8]) -> Vec<u8> {
        [
            map_key(pallet, item, first),
            blake2_128(second).to_vec(),
            second.to_vec(),
        ]
        .concat()
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

    pub fn private_admins(cid: &[u8]) -> Vec<u8> {
        map_key(b"PrivateAdmins", b"AdminAccounts", &cid.to_vec().encode())
    }

    pub fn private_account(cid: &[u8], name: &[u8]) -> Vec<u8> {
        double_map_key(
            b"PrivateManage",
            b"InstitutionAccounts",
            &cid.to_vec().encode(),
            &name.to_vec().encode(),
        )
    }

    pub fn private_reverse(account: &AccountId32) -> Vec<u8> {
        map_key(b"PrivateManage", b"AccountRegisteredCid", &account.encode())
    }

    pub fn user_bank(account: &AccountId32) -> Vec<u8> {
        map_key(OFFCHAIN_PALLET, b"UserBank", &account.encode())
    }

    pub fn deposit(bank_cid: &[u8], account: &AccountId32) -> Vec<u8> {
        // 一级键 = 清算行 CID(InstitutionCidNumber = BoundedVec<u8>),
        // SCALE = Compact(len) || bytes,与 runtime StorageDoubleMap 键逐字节等价。
        double_map_key(
            OFFCHAIN_PALLET,
            b"DepositBalance",
            &bank_cid.to_vec().encode(),
            &account.encode(),
        )
    }

    pub fn bank_total(bank_cid: &[u8]) -> Vec<u8> {
        // 键 = 清算行 CID(Compact(len) || bytes)。
        map_key(
            OFFCHAIN_PALLET,
            b"BankTotalDeposits",
            &bank_cid.to_vec().encode(),
        )
    }

    pub fn last_batch(bank_cid: &[u8]) -> Vec<u8> {
        // 键 = 清算行 CID(Compact(len) || bytes)。
        map_key(
            OFFCHAIN_PALLET,
            b"LastClearingBatchSeq",
            &bank_cid.to_vec().encode(),
        )
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

fn signed_account(xt: &UncheckedExtrinsic) -> Option<[u8; 32]> {
    match &xt.preamble {
        Preamble::Signed(MultiAddress::Id(account), _, _) => Some(account.clone().into()),
        _ => None,
    }
}

fn protected_fee(xt: &UncheckedExtrinsic) -> Option<ProtectedFee> {
    let payer_account_id = signed_account(xt)?;
    match &xt.function {
        RuntimeCall::OnchainTransaction(onchain::pallet::Call::transfer_with_remark {
            amount,
            ..
        }) => Some(ProtectedFee::Onchain {
            payer_account_id,
            amount: *amount,
        }),
        RuntimeCall::InternalVote(internal_vote::pallet::Call::cast { .. })
        | RuntimeCall::JointVote(joint_vote::pallet::Call::cast_admin { .. })
        | RuntimeCall::JointVote(joint_vote::pallet::Call::cast_referendum { .. })
        | RuntimeCall::LegislationVote(
            legislation_vote::pallet::Call::cast_representative_vote { .. }
            | legislation_vote::pallet::Call::cast_referendum_vote { .. }
            | legislation_vote::pallet::Call::executive_sign { .. }
            | legislation_vote::pallet::Call::override_sign { .. }
            | legislation_vote::pallet::Call::guard_vote { .. },
        )
        | RuntimeCall::ElectionVote(
            election_vote::pallet::Call::cast_popular_vote { .. }
            | election_vote::pallet::Call::cast_mutual_vote { .. },
        ) => Some(ProtectedFee::Vote { payer_account_id }),
        _ => None,
    }
}

fn expected_fee(rule: ProtectedFee) -> ([u8; 32], u128) {
    match rule {
        ProtectedFee::Onchain {
            payer_account_id,
            amount,
        } => (
            payer_account_id,
            primitives::fee_policy::calculate_onchain_fee(amount),
        ),
        ProtectedFee::Vote { payer_account_id } => {
            (payer_account_id, primitives::fee_policy::VOTE_FLAT_FEE)
        }
    }
}

fn decode_events<F>(read_post: &F) -> Result<Vec<EventRecord<RuntimeEvent, sp_core::H256>>, String>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    match read_post(&storage_key::events()) {
        Some(raw) => decode_exact(&raw, "System::Events"),
        None => Ok(Vec::new()),
    }
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
pub fn check_block<F>(body: &[sp_runtime::OpaqueExtrinsic], read_post: F) -> Result<(), String>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let extrinsics = decode_body(body)?;
    let events = decode_events(&read_post)?;

    for (index, xt) in extrinsics.iter().enumerate() {
        let Some(rule) = protected_fee(xt) else {
            continue;
        };
        let (expected_payer, expected_amount) = expected_fee(rule);
        let found = events.iter().any(|record| {
            record.phase == Phase::ApplyExtrinsic(index as u32)
                && matches!(
                    &record.event,
                    RuntimeEvent::OnchainTransaction(onchain::pallet::Event::FeePaid {
                        account_id,
                        fee
                    })
                        if <AccountId32 as Clone>::clone(account_id)
                            == AccountId32::new(expected_payer)
                            && *fee == expected_amount
                )
        });
        if !found {
            return Err(format!(
                "第 {index} 笔交易缺少节点固定口径 FeePaid:账户 0x{},金额 {expected_amount} 分",
                hex::encode(expected_payer)
            ));
        }
    }

    for record in &events {
        let RuntimeEvent::OffchainTransaction(offchain::pallet::Event::PaymentSettled {
            recipient_bank_cid,
            transfer_amount,
            fee_amount,
            ..
        }) = &record.event
        else {
            continue;
        };
        let raw = read_post(&storage_key::rate(recipient_bank_cid.as_slice()))
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
pub fn check_transition<F>(
    delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    read_post: F,
) -> Result<(), String>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    check_max_rate(read_post(&storage_key::max_rate()))?;
    let rate_prefix = storage_key::rate_prefix();
    let proposed_prefix = storage_key::proposed_prefix();
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

/// 校验完整导入态中的清算行费率表。
pub fn check_imported_state<'a, I>(entries: I) -> Result<(), String>
where
    I: IntoIterator<Item = (&'a Vec<u8>, &'a Vec<u8>)>,
{
    let state = entries.into_iter().collect::<BTreeMap<_, _>>();
    check_max_rate(
        state
            .get(&storage_key::max_rate())
            .map(|raw| (*raw).clone()),
    )?;
    let rate_prefix = storage_key::rate_prefix();
    let proposed_prefix = storage_key::proposed_prefix();
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

fn overlay_account(
    overlay: &mut OverlayedChanges<BlakeTwo256>,
    account: &[u8; 32],
) -> Result<MAccountInfo, String> {
    let key = fullnode_issuance::storage_key::system_account(account);
    let raw = overlay
        .storage(&key)
        .flatten()
        .ok_or("候选 runtime 未写回合成付款账户")?;
    decode_exact(raw, "候选 System::Account")
}

fn seed_offchain_minimum_fee_probe(
    overlay: &mut OverlayedChanges<BlakeTwo256>,
    submitter_pair: &sr25519::Pair,
) -> Result<(RuntimeCall, AccountId32, Vec<u8>), String> {
    let actor_cid_number = primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: "node-guard-offchain-bank",
            p1: "1",
            province_code: "GD",
            province_name: "广东省",
            city_code: "001",
            city_name: "荔湾市",
            year: "2026",
            institution: "SFGQ",
        },
    )
    .map_err(|error| format!("构造候选 runtime 清算行 CID 失败:{error}"))?
    .into_bytes();
    let submitter = chain_signing::account_id_from_public(submitter_pair.public());
    // 供返回:候选 runtime settlement 后 LastClearingBatchSeq 按 CID 键读取。
    let cid_bytes = actor_cid_number.clone();
    let bank = AccountId32::new([0xB1; 32]);
    let bank_raw: [u8; 32] = bank.clone().into();
    let fee_account = AccountId32::new([0xF1; 32]);
    // 清算账户:Step 2 起 L2 资金落点(充值/提现/结算/偿付);主账户仅身份锚。
    let clearing_account = AccountId32::new([0xC2; 32]);
    let payer_pair = sr25519::Pair::from_seed(&[0xB2; 32]);
    let payer_account_id = chain_signing::account_id_from_public(payer_pair.public());
    let recipient_account_id = AccountId32::new([0xC1; 32]);
    let main_name = primitives::account_derive::RESERVED_NAME_MAIN;
    let fee_name = primitives::account_derive::RESERVED_NAME_FEE;
    let clearing_name = primitives::account_derive::RESERVED_NAME_CLEARING;

    let registered = |name: &[u8]| entity_primitives::RegisteredInstitution {
        cid_number: actor_cid_number.clone(),
        account_name: name.to_vec(),
    };
    let institution_account_id =
        |account_id: AccountId32| entity_primitives::InstitutionAccountInfo {
            account_id,
            initial_balance: 0u128,
            created_at: 0u32,
        };
    overlay.set_storage(
        storage_key::private_account(&actor_cid_number, main_name),
        Some(institution_account_id(bank.clone()).encode()),
    );
    overlay.set_storage(
        storage_key::private_account(&actor_cid_number, fee_name),
        Some(institution_account_id(fee_account.clone()).encode()),
    );
    overlay.set_storage(
        storage_key::private_reverse(&bank),
        Some(registered(main_name).encode()),
    );
    overlay.set_storage(
        storage_key::private_reverse(&fee_account),
        Some(registered(fee_name).encode()),
    );
    // 清算账户正反登记:settlement/deposit/solvency 经 `clearing_account_of(cid)`
    // → `find_account(cid, "清算账户")` 解析资金落点,未登记会 ClearingAccountNotFound。
    overlay.set_storage(
        storage_key::private_account(&actor_cid_number, clearing_name),
        Some(institution_account_id(clearing_account.clone()).encode()),
    );
    overlay.set_storage(
        storage_key::private_reverse(&clearing_account),
        Some(registered(clearing_name).encode()),
    );
    overlay.set_storage(
        storage_key::private_admins(&actor_cid_number),
        Some(
            admin_primitives::InstitutionAdmins {
                institution_code: *b"SFGQ",
                admins: vec![admin_primitives::Admin {
                    account_id: submitter,
                    cid_number: Default::default(),
                    family_name: admin_primitives::FamilyName::truncate_from(
                        admin_primitives::DEFAULT_ADMIN_FAMILY_NAME.to_vec(),
                    ),
                    given_name: admin_primitives::GivenName::truncate_from(
                        admin_primitives::DEFAULT_ADMIN_GIVEN_NAME.to_vec(),
                    ),
                }],
            }
            .encode(),
        ),
    );
    // 身份主键=CID:UserBank 的值是清算行 CID;DepositBalance/BankTotalDeposits/
    // L2FeeRateBp 的键是 CID(Compact(len)||bytes,与 runtime BoundedVec<u8> 键逐字节等价)。
    overlay.set_storage(
        storage_key::user_bank(&payer_account_id),
        Some(actor_cid_number.encode()),
    );
    overlay.set_storage(
        storage_key::user_bank(&recipient_account_id),
        Some(actor_cid_number.encode()),
    );
    overlay.set_storage(
        storage_key::deposit(&actor_cid_number, &payer_account_id),
        Some(2u128.encode()),
    );
    overlay.set_storage(
        storage_key::bank_total(&actor_cid_number),
        Some(2u128.encode()),
    );
    overlay.set_storage(storage_key::rate(&actor_cid_number), Some(10u32.encode()));
    overlay.set_storage(
        fullnode_issuance::storage_key::system_account(&bank_raw),
        Some(
            MAccountInfo {
                nonce: 0,
                consumers: 0,
                providers: 1,
                sufficients: 0,
                data: MAccountData {
                    free: 1_000,
                    reserved: 0,
                    frozen: 0,
                    flags: BALANCES_NEW_ACCOUNT_FLAGS,
                },
            }
            .encode(),
        ),
    );
    let fee_account_raw: [u8; 32] = fee_account.clone().into();
    overlay.set_storage(
        fullnode_issuance::storage_key::system_account(&fee_account_raw),
        Some(
            MAccountInfo {
                nonce: 0,
                consumers: 0,
                providers: 1,
                sufficients: 0,
                data: MAccountData {
                    // 费用账户:先收本批 L2 手续费,再为该批收益付一次链上费(Step 3)。
                    // 给足额度确保链上费扣款成功;80/10/10 分账落点由候选 runtime 基态提供。
                    free: 1_000_000,
                    reserved: 0,
                    frozen: 0,
                    flags: BALANCES_NEW_ACCOUNT_FLAGS,
                },
            }
            .encode(),
        ),
    );
    // 清算账户:L2 资金落点。结算从这里转出手续费,偿付预检读它的余额;给足额度。
    let clearing_raw: [u8; 32] = clearing_account.clone().into();
    overlay.set_storage(
        fullnode_issuance::storage_key::system_account(&clearing_raw),
        Some(
            MAccountInfo {
                nonce: 0,
                consumers: 0,
                providers: 1,
                sufficients: 0,
                data: MAccountData {
                    free: 1_000_000,
                    reserved: 0,
                    frozen: 0,
                    flags: BALANCES_NEW_ACCOUNT_FLAGS,
                },
            }
            .encode(),
        ),
    );

    // item 的 CID 字段填真实清算行 CID(同行探针:付款方=收款方=本行),而非空
    // Default —— 否则与 UserBank/DepositBalance/rate 的 CID 键对不上,settlement 早拒。
    let mut item = offchain::batch_item::OffchainBatchItem::<AccountId32, u32> {
        tx_id: sp_core::H256::repeat_byte(0xD1),
        payer_account_id,
        payer_bank_cid: actor_cid_number
            .clone()
            .try_into()
            .map_err(|_| "候选 runtime 清算行 CID 超长")?,
        recipient_account_id,
        recipient_bank_cid: actor_cid_number
            .clone()
            .try_into()
            .map_err(|_| "候选 runtime 清算行 CID 超长")?,
        transfer_amount: 1,
        fee_amount: primitives::fee_policy::OFFCHAIN_MIN_FEE,
        payer_sig: [0u8; 64],
        payer_nonce: 1,
        expires_at: u32::MAX,
    };
    item.payer_sig = payer_pair.sign(&item.to_intent().signing_hash()).0;
    let batch = vec![item];
    let actor_role_code = b"CLEARING_OPERATOR".to_vec();
    let batch_hash = offchain::batch_item::batch_signing_hash(
        &actor_cid_number,
        &actor_role_code,
        &bank,
        1,
        &batch.encode(),
    );
    let batch_signature = submitter_pair.sign(&batch_hash).0.to_vec();
    let call = RuntimeCall::OffchainTransaction(offchain::pallet::Call::submit_offchain_batch {
        actor_cid_number: actor_cid_number
            .try_into()
            .map_err(|_| "候选 runtime 清算行 CID 超长")?,
        actor_role_code: actor_role_code
            .try_into()
            .map_err(|_| "候选 runtime 清算行岗位码超长")?,
        institution_account_id: bank.clone(),
        batch_seq: 1,
        batch: batch.try_into().map_err(|_| "候选 runtime 清算批次超长")?,
        batch_signature: batch_signature
            .try_into()
            .map_err(|_| "候选 runtime 清算批次签名超长")?,
    });
    Ok((call, bank, cid_bytes))
}

fn set_overlay_value<Value: Encode>(
    overlay: &mut OverlayedChanges<BlakeTwo256>,
    key: Vec<u8>,
    value: &Value,
) {
    overlay.set_storage(key, Some(value.encode()));
}

fn citizen_probe_cid(tag: &str) -> Result<citizen_identity::CidNumberBound, String> {
    primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: tag,
            p1: "1",
            province_code: "ZS",
            province_name: "中枢省",
            city_code: "001",
            city_name: "基准市",
            year: "2027",
            institution: "CTZN",
        },
    )
    .map_err(|error| format!("构造候选 runtime 公民 CID 探针失败:{error}"))?
    .into_bytes()
    .try_into()
    .map_err(|_| "候选 runtime 公民 CID 探针超长".to_string())
}

fn seed_candidate_registrar(
    overlay: &mut OverlayedChanges<BlakeTwo256>,
    registrar: &AccountId32,
) -> Result<
    (
        citizen_identity::CidNumberBound,
        citizen_identity::RoleCodeBound,
    ),
    String,
> {
    let province = primitives::cid::code::PROVINCE_CODE_INFOS
        .first()
        .ok_or("候选 runtime 缺少省级注册局配置")?
        .province_code;
    let institution = primitives::governance_skeleton::federal_registry_institution();
    let actor_cid_number: public_manage::pallet::CidNumberOf<citizenchain::Runtime> = institution
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .map_err(|_| "候选 runtime 注册局 CID 超长")?;
    let admin_cid_number: admin_primitives::AdminCidNumber =
        actor_cid_number
            .to_vec()
            .try_into()
            .map_err(|_| "候选 runtime 管理员注册局 CID 超长")?;
    let actor_role_code: public_manage::RoleCodeOf =
        primitives::governance_skeleton::province_commissioner_role_code(province)
            .try_into()
            .map_err(|_| "候选 runtime 注册局岗位码超长")?;
    let actor_cid_bound: citizen_identity::CidNumberBound = actor_cid_number
        .to_vec()
        .try_into()
        .map_err(|_| "候选 runtime 公民身份注册局 CID 超长")?;
    let actor_role_bound: citizen_identity::RoleCodeBound = actor_role_code
        .to_vec()
        .try_into()
        .map_err(|_| "候选 runtime 公民身份岗位码超长")?;

    let admins: public_admins::pallet::InstitutionAdminsOf<citizenchain::Runtime> =
        admin_primitives::InstitutionAdmins {
            institution_code: admin_primitives::FRG,
            admins: vec![admin_primitives::Admin {
                account_id: registrar.clone(),
                cid_number: Default::default(),
                family_name: Default::default(),
                given_name: Default::default(),
            }]
            .try_into()
            .map_err(|_| "候选 runtime 注册局管理员集合超限")?,
        };
    set_overlay_value(
        overlay,
        public_admins::pallet::AdminAccounts::<citizenchain::Runtime>::hashed_key_for(
            admin_cid_number,
        ),
        &admins,
    );

    let main_account = AccountId32::new(institution.main_account);
    let account_name: public_manage::pallet::AccountNameOf<citizenchain::Runtime> =
        primitives::account_derive::RESERVED_NAME_MAIN
            .to_vec()
            .try_into()
            .map_err(|_| "候选 runtime 注册局主账户名超长")?;
    set_overlay_value(
        overlay,
        public_manage::AccountRegisteredCid::<citizenchain::Runtime>::hashed_key_for(&main_account),
        &entity_primitives::RegisteredInstitution {
            cid_number: actor_cid_number.clone(),
            account_name: account_name.clone(),
        },
    );
    set_overlay_value(
        overlay,
        public_manage::InstitutionAccounts::<citizenchain::Runtime>::hashed_key_for(
            &actor_cid_number,
            &account_name,
        ),
        &entity_primitives::InstitutionAccountInfo {
            account_id: main_account,
            initial_balance: 0u128,
            created_at: 0u32,
        },
    );
    // 注册局业务调用只由管理员签名，交易费必须从 actor CID 的协议费用账户扣取。
    // 探针按正式派生规则建立正反索引并充值，禁止退化为管理员账户代付。
    let fee_account_name: public_manage::pallet::AccountNameOf<citizenchain::Runtime> =
        primitives::account_derive::RESERVED_NAME_FEE
            .to_vec()
            .try_into()
            .map_err(|_| "候选 runtime 注册局费用账户名超长")?;
    let fee_account = AccountId32::new(
        primitives::account_derive::AccountKind::InstitutionFee {
            cid_number: actor_cid_number.as_slice(),
        }
        .derive(primitives::core_const::SS58_FORMAT),
    );
    set_overlay_value(
        overlay,
        public_manage::AccountRegisteredCid::<citizenchain::Runtime>::hashed_key_for(&fee_account),
        &entity_primitives::RegisteredInstitution {
            cid_number: actor_cid_number.clone(),
            account_name: fee_account_name.clone(),
        },
    );
    set_overlay_value(
        overlay,
        public_manage::InstitutionAccounts::<citizenchain::Runtime>::hashed_key_for(
            &actor_cid_number,
            &fee_account_name,
        ),
        &entity_primitives::InstitutionAccountInfo {
            account_id: fee_account.clone(),
            initial_balance: 0u128,
            created_at: 0u32,
        },
    );
    set_overlay_value(
        overlay,
        fullnode_issuance::storage_key::system_account(fee_account.as_ref()),
        &MAccountInfo {
            nonce: 0,
            consumers: 0,
            providers: 1,
            sufficients: 0,
            data: MAccountData {
                free: CITIZEN_SIGNATURE_PROBE_BALANCE,
                reserved: 0,
                frozen: 0,
                flags: BALANCES_NEW_ACCOUNT_FLAGS,
            },
        },
    );
    let institution_info: public_manage::pallet::InstitutionInfoOf<citizenchain::Runtime> =
        entity_primitives::InstitutionInfo {
            cid_full_name: "候选注册局探针"
                .as_bytes()
                .to_vec()
                .try_into()
                .map_err(|_| "候选 runtime 注册局全称超长")?,
            cid_short_name: "注册局探针"
                .as_bytes()
                .to_vec()
                .try_into()
                .map_err(|_| "候选 runtime 注册局简称超长")?,
            town_code: Default::default(),
            legal_representative: None,
            institution_code: admin_primitives::FRG,
            created_at: 0u32,
        };
    set_overlay_value(
        overlay,
        public_manage::Institutions::<citizenchain::Runtime>::hashed_key_for(&actor_cid_number),
        &institution_info,
    );
    let institution_role: public_manage::institution::role::InstitutionRoleOf<
        citizenchain::Runtime,
    > = entity_primitives::InstitutionRole {
        cid_number: actor_cid_number.clone(),
        role_code: actor_role_code.clone(),
        role_name: "候选省专员"
            .as_bytes()
            .to_vec()
            .try_into()
            .map_err(|_| "候选 runtime 注册局岗位名称超长")?,
        term_required: true,
        role_status: entity_primitives::InstitutionRoleStatus::Active,
    };
    set_overlay_value(
        overlay,
        public_manage::InstitutionRoles::<citizenchain::Runtime>::hashed_key_for(
            &actor_cid_number,
            &actor_role_code,
        ),
        &institution_role,
    );
    let assignments: public_manage::institution::role::RoleAssignmentsOf<citizenchain::Runtime> =
        vec![entity_primitives::InstitutionAdminAssignment {
            cid_number: actor_cid_number.clone(),
            account_id: registrar.clone(),
            role_code: actor_role_code.clone(),
            term_start: 1,
            term_end: u32::MAX,
            assignment_source: entity_primitives::InstitutionAssignmentSource::Genesis,
            assignment_source_ref: Default::default(),
            assignment_status: entity_primitives::InstitutionAssignmentStatus::Active,
        }]
        .try_into()
        .map_err(|_| "候选 runtime 注册局任职集合超限")?;
    set_overlay_value(
        overlay,
        public_manage::InstitutionRoleAssignments::<citizenchain::Runtime>::hashed_key_for(
            &actor_cid_number,
            &actor_role_code,
        ),
        &assignments,
    );
    let mut permission_values = Vec::new();
    for spec in entity_primitives::fixed_role_permission_specs(
        admin_primitives::FRG,
        actor_cid_number.as_slice(),
        actor_role_code.as_slice(),
    ) {
        permission_values.push(entity_primitives::RoleBusinessPermission {
            role_subject: entity_primitives::RoleSubject {
                cid_number: actor_cid_number.clone(),
                role_code: actor_role_code.clone(),
            },
            business_action_id: entity_primitives::BusinessActionId {
                module_tag: spec
                    .module_tag
                    .to_vec()
                    .try_into()
                    .map_err(|_| "候选 runtime 注册局业务模块标签超长")?,
                action_code: spec.action_code,
            },
            operation: spec.operation,
        });
    }
    let permissions: public_manage::RolePermissionsOf<citizenchain::Runtime> = permission_values
        .try_into()
        .map_err(|_| "候选 runtime 注册局权限集合超限")?;
    set_overlay_value(
        overlay,
        public_manage::InstitutionRolePermissions::<citizenchain::Runtime>::hashed_key_for(
            &actor_cid_number,
            &actor_role_code,
        ),
        &permissions,
    );
    Ok((actor_cid_bound, actor_role_bound))
}

fn seed_occupied_citizen_cid(
    overlay: &mut OverlayedChanges<BlakeTwo256>,
    actor_cid_number: &citizen_identity::CidNumberBound,
    cid_number: &citizen_identity::CidNumberBound,
    account_id: &AccountId32,
    block_number: u32,
) {
    let record = citizen_identity::CidRecord {
        registrar_cid_number: actor_cid_number.clone(),
        commitment: sp_io::hashing::blake2_256(&account_id.encode()),
        residence_province_code: Default::default(),
        residence_city_code: Default::default(),
        status: citizen_identity::CidRecordStatus::Active,
        registered_at: block_number,
        revoked_at: None,
    };
    set_overlay_value(
        overlay,
        citizen_identity::CidRegistry::<citizenchain::Runtime>::hashed_key_for(cid_number),
        &record,
    );
    set_overlay_value(
        overlay,
        citizen_identity::AccountIdByCid::<citizenchain::Runtime>::hashed_key_for(cid_number),
        account_id,
    );
    set_overlay_value(
        overlay,
        citizen_identity::CidByAccountId::<citizenchain::Runtime>::hashed_key_for(account_id),
        cid_number,
    );
    set_overlay_value(
        overlay,
        citizen_identity::BindingRevisionByCid::<citizenchain::Runtime>::hashed_key_for(cid_number),
        &1u64,
    );
}

fn candidate_citizen_signature(
    signer: &sr25519::Pair,
    op_tag: u8,
    payload: &[u8],
) -> Result<citizen_identity::pallet::SignatureOf<citizenchain::Runtime>, String> {
    signer
        .sign(&primitives::sign::signing_message(op_tag, payload))
        .0
        .to_vec()
        .try_into()
        .map_err(|_| "候选 runtime sr25519 签名超过公民身份边界".to_string())
}

fn forged_citizen_signature(
) -> Result<citizen_identity::pallet::SignatureOf<citizenchain::Runtime>, String> {
    [0xA5u8; 64]
        .to_vec()
        .try_into()
        .map_err(|_| "候选 runtime 伪造签名超过公民身份边界".to_string())
}

#[allow(clippy::too_many_arguments)]
fn apply_citizen_signature_probe<B>(
    backend: &B,
    overlay: &mut OverlayedChanges<BlakeTwo256>,
    executor: &sc_executor::WasmExecutor<sp_io::SubstrateHostFunctions>,
    runtime_code: &sp_core::traits::RuntimeCode<'_>,
    parent_hash: sp_core::H256,
    genesis_hash: sp_core::H256,
    version: &sc_executor::RuntimeVersion,
    signer: &sr25519::Pair,
    nonce: u32,
    call: RuntimeCall,
    expected_error: Option<sp_runtime::DispatchError>,
    label: &str,
) -> Result<(), String>
where
    B: Backend<BlakeTwo256>,
{
    let extrinsic = chain_signing::build_signed_extrinsic_with_pair(
        call,
        genesis_hash,
        nonce,
        version.spec_version,
        version.transaction_version,
        signer,
    );
    let result = call_candidate(
        backend,
        overlay,
        executor,
        runtime_code,
        parent_hash,
        "BlockBuilder_apply_extrinsic",
        &extrinsic.encode(),
    )?;
    let result: sp_runtime::ApplyExtrinsicResult =
        decode_exact(&result, "候选公民身份 ApplyExtrinsicResult")?;
    match (result, expected_error) {
        (Ok(Ok(())), None) => Ok(()),
        (Ok(Err(actual)), Some(expected)) if actual == expected => Ok(()),
        (actual, expected) => Err(format!(
            "候选 runtime 公民身份验签探针 {label} 结果错误:期望 {expected:?},实际 {actual:?}"
        )),
    }
}

#[allow(clippy::too_many_arguments)]
fn check_candidate_citizen_identity_signatures<B>(
    backend: &B,
    executor: &sc_executor::WasmExecutor<sp_io::SubstrateHostFunctions>,
    runtime_code: &sp_core::traits::RuntimeCode<'_>,
    parent_hash: sp_core::H256,
    next_header: &citizenchain::Header,
    post_delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    version: &sc_executor::RuntimeVersion,
    registrar_pair: &sr25519::Pair,
    genesis_hash: sp_core::H256,
) -> Result<(), String>
where
    B: Backend<BlakeTwo256>,
{
    let mut overlay = OverlayedChanges::<BlakeTwo256>::default();
    for (key, value) in post_delta {
        overlay.set_storage(key.clone(), value.clone());
    }
    let registrar = AccountId32::new(registrar_pair.public().0);
    let payer_key = fullnode_issuance::storage_key::system_account(registrar.as_ref());
    set_overlay_value(
        &mut overlay,
        payer_key,
        &MAccountInfo {
            nonce: 0,
            consumers: 0,
            providers: 1,
            sufficients: 0,
            data: MAccountData {
                free: CITIZEN_SIGNATURE_PROBE_BALANCE,
                reserved: 0,
                frozen: 0,
                flags: BALANCES_NEW_ACCOUNT_FLAGS,
            },
        },
    );
    set_overlay_value(
        &mut overlay,
        pallet_timestamp::Now::<citizenchain::Runtime>::hashed_key().to_vec(),
        &1_800_000_000_000u64,
    );
    set_overlay_value(
        &mut overlay,
        citizen_identity::PopulationReadyDate::<citizenchain::Runtime>::hashed_key().to_vec(),
        &20270115u32,
    );
    overlay.set_storage(
        citizen_identity::PopulationMaintenanceFault::<citizenchain::Runtime>::hashed_key()
            .to_vec(),
        None,
    );
    let (actor_cid_number, actor_role_code) = seed_candidate_registrar(&mut overlay, &registrar)?;
    call_candidate(
        backend,
        &mut overlay,
        executor,
        runtime_code,
        parent_hash,
        "Core_initialize_block",
        &next_header.encode(),
    )?;

    let expires_at = 1_800_000_300u64;
    let forged = forged_citizen_signature()?;
    let mut nonce = 0u32;

    // 注册局占号：授权与 CID 前置条件完全相同，只替换内层签名。
    let occupy_pair = sr25519::Pair::from_seed(&[0x41; 32]);
    let occupy_account_id = AccountId32::new(occupy_pair.public().0);
    let occupy_cid_number = citizen_probe_cid("node-guard-occupy")?;
    let occupy_payload = citizen_identity::CidOccupyAuthorization {
        genesis_hash,
        cid_number: occupy_cid_number.clone(),
        account_id: occupy_account_id.clone(),
        expected_binding_revision: 0,
        expires_at,
    }
    .encode();
    let valid_occupy_signature = candidate_citizen_signature(
        &occupy_pair,
        primitives::sign::OP_SIGN_CID_OCCUPY,
        &occupy_payload,
    )?;
    let occupy_call = |citizen_signature| {
        RuntimeCall::CitizenIdentity(citizen_identity::pallet::Call::occupy_cid {
            actor_cid_number: actor_cid_number.clone(),
            actor_role_code: actor_role_code.clone(),
            cid_number: occupy_cid_number.clone(),
            account_id: occupy_account_id.clone(),
            expires_at,
            citizen_signature,
        })
    };
    apply_citizen_signature_probe(
        backend,
        &mut overlay,
        executor,
        runtime_code,
        parent_hash,
        genesis_hash,
        version,
        registrar_pair,
        nonce,
        occupy_call(forged.clone()),
        Some(citizen_identity::Error::<citizenchain::Runtime>::InvalidOccupySignature.into()),
        "occupy_cid/forged",
    )?;
    nonce = nonce.saturating_add(1);
    if overlay
        .storage(
            &citizen_identity::AccountIdByCid::<citizenchain::Runtime>::hashed_key_for(
                &occupy_cid_number,
            ),
        )
        .flatten()
        .is_some()
    {
        return Err("候选 runtime 伪造占号签名修改了 CID 绑定".to_string());
    }
    apply_citizen_signature_probe(
        backend,
        &mut overlay,
        executor,
        runtime_code,
        parent_hash,
        genesis_hash,
        version,
        registrar_pair,
        nonce,
        occupy_call(valid_occupy_signature),
        None,
        "occupy_cid/valid",
    )?;
    nonce = nonce.saturating_add(1);

    // 公民投票身份登记：先种入占号绑定，伪造签名必须在正式 citizen 域被拒绝。
    let citizen_pair = sr25519::Pair::from_seed(&[0x42; 32]);
    let citizen_account_id = AccountId32::new(citizen_pair.public().0);
    let citizen_cid_number = citizen_probe_cid("node-guard-citizen")?;
    seed_occupied_citizen_cid(
        &mut overlay,
        &actor_cid_number,
        &citizen_cid_number,
        &citizen_account_id,
        *next_header.number(),
    );
    let voting_payload = citizen_identity::VotingIdentityPayload {
        cid_number: citizen_cid_number.clone(),
        account_id: citizen_account_id,
        passport_valid_from: 20270101,
        passport_valid_until: 20991231,
        citizen_status: citizen_identity::CitizenStatus::Normal,
        residence_province_code: b"ZS".to_vec().try_into().map_err(|_| "省码超长")?,
        residence_city_code: b"ZS01".to_vec().try_into().map_err(|_| "市码超长")?,
        residence_town_code: b"ZS01001".to_vec().try_into().map_err(|_| "镇码超长")?,
    };
    // 防重放三件套:公民签的是**完整授权**而不是裸 payload,与 pallet 的
    // `citizen_identity_authorization` 逐字节同构(genesis_hash ++ payload ++ 版本 ++ 过期)。
    // 签死裸 payload 会让本探针的 "valid" 用例反被判 InvalidCitizenSignature,
    // 从而把守卫变成永远报 KnownBad 的假阳性。
    //
    // 版本取 0:`seed_occupied_citizen_cid` 只种占号绑定,新 CID 的
    // `VotingEligibilityVersionCount` 保持默认 0,必须与链端读到的值相等才过
    // `ensure_expected_identity_version`(它在验签**之前**执行,填错会拿到
    // IdentityVersionMismatch 而非签名错误,伪造用例的断言就会失真)。
    let voting_identity_version = 0u64;
    let voting_authorization = citizen_identity::CitizenIdentityAuthorization {
        genesis_hash,
        payload: voting_payload.clone(),
        expected_identity_version: voting_identity_version,
        expires_at,
    }
    .encode();
    let valid_citizen_signature = candidate_citizen_signature(
        &citizen_pair,
        primitives::sign::OP_SIGN_CITIZEN_IDENTITY,
        &voting_authorization,
    )?;
    let voting_call = |citizen_signature| {
        RuntimeCall::CitizenIdentity(citizen_identity::pallet::Call::register_voting_identity {
            actor_cid_number: actor_cid_number.clone(),
            actor_role_code: actor_role_code.clone(),
            payload: voting_payload.clone(),
            expected_identity_version: voting_identity_version,
            expires_at,
            citizen_signature,
        })
    };
    apply_citizen_signature_probe(
        backend,
        &mut overlay,
        executor,
        runtime_code,
        parent_hash,
        genesis_hash,
        version,
        registrar_pair,
        nonce,
        voting_call(forged.clone()),
        Some(citizen_identity::Error::<citizenchain::Runtime>::InvalidCitizenSignature.into()),
        "register_voting_identity/forged",
    )?;
    nonce = nonce.saturating_add(1);
    if overlay
        .storage(&citizen_identity::VotingIdentityByCid::<
            citizenchain::Runtime,
        >::hashed_key_for(&citizen_cid_number))
        .flatten()
        .is_some()
    {
        return Err("候选 runtime 伪造公民签名写入了投票身份".to_string());
    }
    apply_citizen_signature_probe(
        backend,
        &mut overlay,
        executor,
        runtime_code,
        parent_hash,
        genesis_hash,
        version,
        registrar_pair,
        nonce,
        voting_call(valid_citizen_signature),
        None,
        "register_voting_identity/valid",
    )?;
    nonce = nonce.saturating_add(1);

    // 注册局换绑：新账户的 admin-rebind 域签名必须真实有效。
    let admin_current_pair = sr25519::Pair::from_seed(&[0x43; 32]);
    let admin_current_account_id = AccountId32::new(admin_current_pair.public().0);
    let admin_new_pair = sr25519::Pair::from_seed(&[0x44; 32]);
    let admin_new_account_id = AccountId32::new(admin_new_pair.public().0);
    let admin_cid_number = citizen_probe_cid("node-guard-admin-rebind")?;
    seed_occupied_citizen_cid(
        &mut overlay,
        &actor_cid_number,
        &admin_cid_number,
        &admin_current_account_id,
        *next_header.number(),
    );
    let admin_payload = citizen_identity::CidRebindAuthorization {
        genesis_hash,
        cid_number: admin_cid_number.clone(),
        current_account_id: admin_current_account_id.clone(),
        new_account_id: admin_new_account_id.clone(),
        expected_binding_revision: 1,
        expires_at,
    }
    .encode();
    let valid_admin_signature = candidate_citizen_signature(
        &admin_new_pair,
        primitives::sign::OP_SIGN_CID_ADMIN_REBIND,
        &admin_payload,
    )?;
    let admin_call = |new_account_signature| {
        RuntimeCall::CitizenIdentity(
            citizen_identity::pallet::Call::admin_rebind_cid_account_id {
                actor_cid_number: actor_cid_number.clone(),
                actor_role_code: actor_role_code.clone(),
                cid_number: admin_cid_number.clone(),
                new_account_id: admin_new_account_id.clone(),
                expected_binding_revision: 1,
                expires_at,
                new_account_signature,
            },
        )
    };
    apply_citizen_signature_probe(
        backend,
        &mut overlay,
        executor,
        runtime_code,
        parent_hash,
        genesis_hash,
        version,
        registrar_pair,
        nonce,
        admin_call(forged.clone()),
        Some(citizen_identity::Error::<citizenchain::Runtime>::InvalidAdminRebindSignature.into()),
        "admin_rebind_cid_account_id/forged",
    )?;
    nonce = nonce.saturating_add(1);
    let admin_binding_key =
        citizen_identity::AccountIdByCid::<citizenchain::Runtime>::hashed_key_for(
            &admin_cid_number,
        );
    let binding: AccountId32 = decode_exact(
        overlay
            .storage(&admin_binding_key)
            .flatten()
            .ok_or("候选 runtime 丢失注册局换绑探针绑定")?,
        "候选注册局换绑当前账户",
    )?;
    if binding != admin_current_account_id {
        return Err("候选 runtime 伪造注册局换绑签名改变了当前账户".to_string());
    }
    apply_citizen_signature_probe(
        backend,
        &mut overlay,
        executor,
        runtime_code,
        parent_hash,
        genesis_hash,
        version,
        registrar_pair,
        nonce,
        admin_call(valid_admin_signature),
        None,
        "admin_rebind_cid_account_id/valid",
    )?;
    nonce = nonce.saturating_add(1);

    // 自主换绑：当前绑定账户签名，交易由新账户（本探针 registrar_pair）提交。
    let self_current_pair = sr25519::Pair::from_seed(&[0x45; 32]);
    let self_current_account_id = AccountId32::new(self_current_pair.public().0);
    let self_cid_number = citizen_probe_cid("node-guard-self-rebind")?;
    seed_occupied_citizen_cid(
        &mut overlay,
        &actor_cid_number,
        &self_cid_number,
        &self_current_account_id,
        *next_header.number(),
    );
    let self_payload = citizen_identity::CidRebindAuthorization {
        genesis_hash,
        cid_number: self_cid_number.clone(),
        current_account_id: self_current_account_id.clone(),
        new_account_id: registrar.clone(),
        expected_binding_revision: 1,
        expires_at,
    }
    .encode();
    let valid_self_signature = candidate_citizen_signature(
        &self_current_pair,
        primitives::sign::OP_SIGN_CID_REBIND,
        &self_payload,
    )?;
    let self_call = |current_account_signature| {
        RuntimeCall::CitizenIdentity(citizen_identity::pallet::Call::self_rebind_cid_account_id {
            cid_number: self_cid_number.clone(),
            expected_binding_revision: 1,
            expires_at,
            current_account_signature,
        })
    };
    apply_citizen_signature_probe(
        backend,
        &mut overlay,
        executor,
        runtime_code,
        parent_hash,
        genesis_hash,
        version,
        registrar_pair,
        nonce,
        self_call(forged),
        Some(citizen_identity::Error::<citizenchain::Runtime>::InvalidRebindSignature.into()),
        "self_rebind_cid_account_id/forged",
    )?;
    nonce = nonce.saturating_add(1);
    let self_binding_key =
        citizen_identity::AccountIdByCid::<citizenchain::Runtime>::hashed_key_for(&self_cid_number);
    let binding: AccountId32 = decode_exact(
        overlay
            .storage(&self_binding_key)
            .flatten()
            .ok_or("候选 runtime 丢失自主换绑探针绑定")?,
        "候选自主换绑当前账户",
    )?;
    if binding != self_current_account_id {
        return Err("候选 runtime 伪造自主换绑签名改变了当前账户".to_string());
    }
    apply_citizen_signature_probe(
        backend,
        &mut overlay,
        executor,
        runtime_code,
        parent_hash,
        genesis_hash,
        version,
        registrar_pair,
        nonce,
        self_call(valid_self_signature),
        None,
        "self_rebind_cid_account_id/valid",
    )?;
    Ok(())
}

/// 在候选 WASM 生效前执行固定费用行为探针。
///
/// 这不是 WASM 哈希白名单，也不要求升级 runtime 随节点发布；任意候选代码都只按
/// 实际行为判定合法或非法。
pub fn check_candidate_runtime<B>(
    backend: &B,
    parent_hash: sp_core::H256,
    current_hash: sp_core::H256,
    current_number: u32,
    post_delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    candidate_code: &[u8],
    genesis_hash: sp_core::H256,
) -> Result<(), String>
where
    B: Backend<BlakeTwo256>,
{
    use std::borrow::Cow;

    let mut overlay = OverlayedChanges::<BlakeTwo256>::default();
    for (key, value) in post_delta {
        overlay.set_storage(key.clone(), value.clone());
    }

    let payer_pair = sr25519::Pair::from_seed(&[0xA5; 32]);
    let payer_account_id: [u8; 32] = payer_pair.public().into();
    let recipient_account_id = AccountId32::new([0x5A; 32]);
    let payer_key = fullnode_issuance::storage_key::system_account(&payer_account_id);
    let payer_info = MAccountInfo {
        nonce: 0,
        consumers: 0,
        providers: 1,
        sufficients: 0,
        data: MAccountData {
            free: SYNTHETIC_BALANCE,
            reserved: 0,
            frozen: 0,
            flags: BALANCES_NEW_ACCOUNT_FLAGS,
        },
    };
    overlay.set_storage(payer_key, Some(payer_info.encode()));

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
        <dyn frame_benchmarking::Benchmark<citizenchain::Block> as sp_api::RuntimeApiInfo>::ID;
    if version
        .apis
        .iter()
        .any(|(api_id, _)| api_id == &benchmark_api_id)
    {
        return Err("候选 runtime 暴露 Benchmark runtime API，禁止作为正式 :code".to_string());
    }

    let next_header = citizenchain::Header::new(
        current_number.saturating_add(1),
        Default::default(),
        Default::default(),
        current_hash,
        Default::default(),
    );
    check_candidate_citizen_identity_signatures(
        backend,
        &executor,
        &runtime_code,
        parent_hash,
        &next_header,
        post_delta,
        &version,
        &payer_pair,
        genesis_hash,
    )?;
    call_candidate(
        backend,
        &mut overlay,
        &executor,
        &runtime_code,
        parent_hash,
        "Core_initialize_block",
        &next_header.encode(),
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

    let probes = [
        (
            RuntimeCall::OnchainTransaction(onchain::pallet::Call::transfer_with_remark {
                beneficiary_account_id: recipient_account_id.clone(),
                amount: 50_000,
                remark: b"node-guard-rate"
                    .to_vec()
                    .try_into()
                    .map_err(|_| "构造候选 runtime 费率探针备注失败")?,
            }),
            50_050u128,
        ),
        (
            RuntimeCall::OnchainTransaction(onchain::pallet::Call::transfer_with_remark {
                beneficiary_account_id: recipient_account_id,
                amount: 1,
                remark: b"node-guard-min"
                    .to_vec()
                    .try_into()
                    .map_err(|_| "构造候选 runtime 最低费探针备注失败")?,
            }),
            11u128,
        ),
        (
            RuntimeCall::InternalVote(internal_vote::pallet::Call::cast {
                proposal_id: u64::MAX,
                ticket_claim: internal_vote::InternalVoteTicketClaim::Personal,
                approve: true,
            }),
            primitives::fee_policy::VOTE_FLAT_FEE,
        ),
    ];

    let mut expected_free = SYNTHETIC_BALANCE;
    for (nonce, (call, expected_debit)) in probes.into_iter().enumerate() {
        let xt = chain_signing::build_signed_extrinsic_with_pair(
            call,
            genesis_hash,
            nonce as u32,
            version.spec_version,
            version.transaction_version,
            &payer_pair,
        );
        call_candidate(
            backend,
            &mut overlay,
            &executor,
            &runtime_code,
            parent_hash,
            "BlockBuilder_apply_extrinsic",
            &xt.encode(),
        )?;
        expected_free = expected_free
            .checked_sub(expected_debit)
            .ok_or("候选 runtime 费用探针余额下溢")?;
        let actual = overlay_account(&mut overlay, &payer_account_id)?.data.free;
        if actual != expected_free {
            return Err(format!(
                "候选 runtime 费用行为非法:第 {nonce} 个探针期望余额 {expected_free},实际 {actual}"
            ));
        }
    }

    // 用同一候选 WASM 真实走完整清算批次，1 分交易在 0.1% 费率下必须收取最低 1 分。
    // 成功写入批次序号同时证明公式校验、L3 签名、批次签名和资金落账均已通过。
    let (offchain_call, _bank, bank_cid) =
        seed_offchain_minimum_fee_probe(&mut overlay, &payer_pair)?;
    let xt = chain_signing::build_signed_extrinsic_with_pair(
        offchain_call,
        genesis_hash,
        3,
        version.spec_version,
        version.transaction_version,
        &payer_pair,
    );
    let apply_result = call_candidate(
        backend,
        &mut overlay,
        &executor,
        &runtime_code,
        parent_hash,
        "BlockBuilder_apply_extrinsic",
        &xt.encode(),
    )?;
    let apply_result: sp_runtime::ApplyExtrinsicResult =
        decode_exact(&apply_result, "候选链下清算 ApplyExtrinsicResult")?;
    let call_filtered: sp_runtime::DispatchError =
        frame_system::Error::<citizenchain::Runtime>::CallFiltered.into();
    match apply_result {
        Ok(Ok(())) => {}
        // 当前链下交易 pallet 尚未对外启用，BaseCallFilter 会明确拒绝该入口。
        // 此时没有可执行的链下收费路径；以后候选 runtime 一旦开放该入口，
        // 同一探针必须真实结算成功，最低 1 分的制度才允许随区块生效。
        Ok(Err(error)) if error == call_filtered => return Ok(()),
        other => {
            return Err(format!("候选 runtime 链下清算探针执行失败:{other:?}"));
        }
    }
    let last_batch_raw = overlay
        .storage(&storage_key::last_batch(&bank_cid))
        .flatten()
        .ok_or("候选 runtime 未完成链下最低手续费清算探针")?;
    let last_batch: u64 = decode_exact(last_batch_raw, "候选 LastClearingBatchSeq")?;
    if last_batch != 1 {
        return Err(format!(
            "候选 runtime 链下最低手续费行为非法:批次序号应为 1,实际 {last_batch}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_transfer(amount: u128) -> (Vec<sp_runtime::OpaqueExtrinsic>, AccountId32) {
        let pair = sr25519::Pair::from_seed(&[7u8; 32]);
        let payer_account_id = chain_signing::account_id_from_public(pair.public());
        let call = RuntimeCall::OnchainTransaction(onchain::pallet::Call::transfer_with_remark {
            beneficiary_account_id: AccountId32::new([8u8; 32]),
            amount,
            remark: b"fee-test".to_vec().try_into().expect("remark fits"),
        });
        let xt = chain_signing::build_signed_extrinsic_local(
            call,
            sp_core::H256::repeat_byte(1),
            0,
            &pair,
        );
        (vec![xt.into()], payer_account_id)
    }

    #[test]
    fn node_fixed_fee_values_are_exact() {
        assert_eq!(primitives::fee_policy::calculate_onchain_fee(1), 10);
        assert_eq!(primitives::fee_policy::calculate_onchain_fee(50_000), 50);
        assert_eq!(primitives::fee_policy::VOTE_FLAT_FEE, 100);
        assert_eq!(primitives::fee_policy::OFFCHAIN_MIN_FEE, 1);
        assert_eq!(offchain_fee(50_000, OFFCHAIN_MAX_RATE_UNITS), 50);
    }

    #[test]
    fn rates_above_zero_point_one_percent_are_rejected() {
        assert!(check_rate_value(0).is_ok());
        assert!(check_rate_value(10).is_ok());
        assert!(check_rate_value(11).is_err());
        assert!(check_max_rate(Some(11u32.encode())).is_err());
    }

    #[test]
    fn actual_onchain_fee_event_must_match_fixed_formula() {
        let (body, payer_account_id) = signed_transfer(50_000);
        let good_events: Vec<EventRecord<RuntimeEvent, sp_core::H256>> = vec![EventRecord {
            phase: Phase::ApplyExtrinsic(0),
            event: RuntimeEvent::OnchainTransaction(onchain::pallet::Event::FeePaid {
                account_id: payer_account_id.clone(),
                fee: 50,
            }),
            topics: Vec::new(),
        }];
        let events_key = storage_key::events();
        assert!(check_block(&body, |key| {
            (key == events_key.as_slice()).then(|| good_events.encode())
        })
        .is_ok());

        let bad_events: Vec<EventRecord<RuntimeEvent, sp_core::H256>> = vec![EventRecord {
            phase: Phase::ApplyExtrinsic(0),
            event: RuntimeEvent::OnchainTransaction(onchain::pallet::Event::FeePaid {
                account_id: payer_account_id,
                fee: 49,
            }),
            topics: Vec::new(),
        }];
        assert!(check_block(&body, |key| {
            (key == events_key.as_slice()).then(|| bad_events.encode())
        })
        .is_err());
    }

    #[test]
    fn actual_offchain_settlement_uses_recipient_rate_and_one_fen_minimum() {
        let bank: Vec<u8> = b"ZS001-PRB08-233384677-2026".to_vec();
        let events: Vec<EventRecord<RuntimeEvent, sp_core::H256>> = vec![EventRecord {
            phase: Phase::ApplyExtrinsic(0),
            event: RuntimeEvent::OffchainTransaction(offchain::pallet::Event::PaymentSettled {
                tx_id: sp_core::H256::repeat_byte(4),
                payer_account_id: AccountId32::new([5u8; 32]),
                payer_bank_cid: b"GD001-PRB0T-239565809-2026".to_vec().try_into().unwrap(),
                recipient_account_id: AccountId32::new([7u8; 32]),
                recipient_bank_cid: bank.clone().try_into().unwrap(),
                transfer_amount: 1,
                fee_amount: 1,
            }),
            topics: Vec::new(),
        }];
        let events_key = storage_key::events();
        let rate_key = storage_key::rate(&bank);
        assert!(check_block(&[], |key| {
            if key == events_key.as_slice() {
                Some(events.encode())
            } else if key == rate_key.as_slice() {
                Some(10u32.encode())
            } else {
                None
            }
        })
        .is_ok());

        let wrong_events: Vec<EventRecord<RuntimeEvent, sp_core::H256>> = vec![EventRecord {
            phase: Phase::ApplyExtrinsic(0),
            event: RuntimeEvent::OffchainTransaction(offchain::pallet::Event::PaymentSettled {
                tx_id: sp_core::H256::repeat_byte(4),
                payer_account_id: AccountId32::new([5u8; 32]),
                payer_bank_cid: b"GD001-PRB0T-239565809-2026".to_vec().try_into().unwrap(),
                recipient_account_id: AccountId32::new([7u8; 32]),
                recipient_bank_cid: bank.try_into().unwrap(),
                transfer_amount: 1,
                fee_amount: 2,
            }),
            topics: Vec::new(),
        }];
        assert!(check_block(&[], |key| {
            if key == events_key.as_slice() {
                Some(wrong_events.encode())
            } else if key == rate_key.as_slice() {
                Some(10u32.encode())
            } else {
                None
            }
        })
        .is_err());
    }

    /// 字节锁:清算体系 CID 存储键必须 = twox_128(pallet) || twox_128(item)
    /// || blake2_128(SCALE(cid)) || SCALE(cid),其中 SCALE(cid) = Compact(len)||bytes
    /// (与 runtime `StorageMap<_, Blake2_128Concat, InstitutionCidNumber, _>` 逐字节一致)。
    /// 若回退成旧的定长 32B 账户编码(节点会静默读空),本测试必红。
    #[test]
    fn cid_storage_keys_use_compact_len_prefix() {
        let cid: Vec<u8> = b"AH001-SCB05-000000002-2026".to_vec();
        let encoded = cid.encode(); // Compact(26)=0x68 || 26 bytes
        assert_eq!(encoded[0], 0x68, "26<<2 = 0x68 单字节 compact 前缀");
        assert_eq!(&encoded[1..], &cid[..]);

        let mut want = Vec::new();
        want.extend_from_slice(&sp_io::hashing::twox_128(b"OffchainTransaction"));
        want.extend_from_slice(&sp_io::hashing::twox_128(b"LastClearingBatchSeq"));
        want.extend_from_slice(&sp_io::hashing::blake2_128(&encoded));
        want.extend_from_slice(&encoded);
        assert_eq!(storage_key::last_batch(&cid), want);

        // BankTotalDeposits 键尾同为 Compact(len)||bytes CID。
        assert!(storage_key::bank_total(&cid).ends_with(&encoded));

        // DepositBalance 双 map:一级 CID 段(变长)必现于键内,证明非定长 32B 账户编码。
        let acc = AccountId32::new([0x09; 32]);
        assert!(storage_key::deposit(&cid, &acc)
            .windows(encoded.len())
            .any(|w| w == encoded.as_slice()));
    }
}

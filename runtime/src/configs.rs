// This is free and unencumbered software released into the public domain.
//
// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.
//
// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.
//
// For more information, please refer to <http://unlicense.org>

//! 装配各pallet配置、费用路由、权限和运行时迁移。

use alloc::vec::Vec;
use codec::Decode;
use entitybase::InstitutionMultisigQuery;
use entitybase::{
    InstitutionRoleAuthorizationQuery, InstitutionRoleQuery, RolePermissionOperation,
};
#[cfg(not(feature = "runtime-benchmarks"))]
use frame_support::traits::UnfilteredDispatchable;
use frame_support::{
    derive_impl,
    dispatch::{DispatchClass, DispatchResult},
    parameter_types,
    storage::types::{OptionQuery, StorageMap},
    traits::{
        fungible::{Balanced, Credit},
        ConstBool, ConstU128, ConstU32, ConstU64, ConstU8, Contains, EnsureOrigin, FindAuthor,
        OnKilledAccount, OnRuntimeUpgrade, OnUnbalanced, StorageInstance, StoredMap,
        VariantCountOf,
    },
    Blake2_128Concat,
    weights::{
        constants::{RocksDbWeight, WEIGHT_REF_TIME_PER_SECOND},
        ConstantMultiplier, Weight,
    },
};
use frame_system::limits::{BlockLength, BlockWeights};
use onchain::NrcAccountProvider as _;
use pallet_transaction_payment::{ConstFeeMultiplier, Multiplier};
#[cfg(feature = "runtime-benchmarks")]
use sp_core::sr25519;
use sp_core::Void;
#[cfg(feature = "runtime-benchmarks")]
use sp_runtime::{traits::IdentifyAccount, MultiSigner};
use sp_runtime::{traits::One, Perbill};
use sp_version::RuntimeVersion;

// Local module imports
use super::{
    AccountId, Assets, Balance, Balances, Block, BlockNumber, CitizenIssue, ElectionVote,
    Genesis, Hash, InternalVote, JointVote, LegislationVote, Legislation, Nonce,
    PalletInfo, PrivateEntity, PublicEntity, Runtime, RuntimeCall,
    RuntimeEvent, RuntimeFreezeReason, RuntimeHoldReason, RuntimeOrigin, RuntimeTask, System,
    BLOCK_HASH_COUNT, EXISTENTIAL_DEPOSIT, VERSION,
};
#[cfg(not(feature = "runtime-benchmarks"))]
use super::{ResolutionIssue, Upgrade};

const NORMAL_DISPATCH_RATIO: Perbill =
    Perbill::from_percent(primitives::core_const::NORMAL_DISPATCH_PERCENT);

parameter_types! {
    pub const BlockHashCount: BlockNumber = BLOCK_HASH_COUNT;
    /// 使用 BlockNumber 类型声明重试宽限期，避免与具体 u32 常量类型耦合。
    pub const VotingExecutionRetryGraceBlocks: BlockNumber = 21_600;
    pub VotingMaintenanceBudget: Weight = {
        let max=RuntimeBlockWeights::get().max_block;
        Weight::from_parts(max.ref_time()/4*3,max.proof_size()/4*3)
    };
    pub const Version: RuntimeVersion = VERSION;

    /// 每个区块允许 60 秒计算预算及 8MiB 空间预算；资源限额不参与费用计算。
    pub RuntimeBlockWeights: BlockWeights = BlockWeights::with_sensible_defaults(
        Weight::from_parts(60u64 * WEIGHT_REF_TIME_PER_SECOND, 8 * 1024 * 1024),
        NORMAL_DISPATCH_RATIO,
    );
    pub RuntimeBlockLength: BlockLength = BlockLength::builder()
        .max_length(primitives::core_const::MAX_BLOCK_BYTES)
        .modify_max_length_for_class(
            DispatchClass::Normal,
            |max_length| *max_length = NORMAL_DISPATCH_RATIO * *max_length,
        )
        .build();
    /// 公民币主链地址编号（SS58 前缀）：统一来源于 primitives 常量。
    pub const SS58Prefix: u16 = primitives::core_const::SS58_FORMAT;
}

/// 现有链上的固定委员岗位权限由 Runtime 升级补齐。
type SingleBlockMigrations = (
    crate::migrations::RenamePallets,
    crate::migrations::Institutions,
    AddPrcFeeSweepPermissions,
    crate::migrations::GrandpaKeys,
    crate::migrations::EvmBackfill,
);

const PRC_FEE_SWEEP_MIGRATED: &[u8] = b":citizenchain:prc-fee-sweep-permissions:v1";

type PrcFeeSweepPermissionUpdates = Vec<(
    publicentity::pallet::CidNumberOf<Runtime>,
    publicentity::RolePermissionsOf<Runtime>,
)>;

pub struct AddPrcFeeSweepPermissions;

impl AddPrcFeeSweepPermissions {
    fn prepare() -> Result<(publicentity::RoleCodeOf, PrcFeeSweepPermissionUpdates), &'static str> {
        use entitybase::business_action::{ACTION_FEE_SWEEP_TO_MAIN, MODULE_MULTISIG};
        use primitives::cid::china::china_cb::CHINA_CB;
        use primitives::cid::code::{institution_code_from_cid_number, PRC};
        use primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER;

        let role_code: publicentity::RoleCodeOf = ROLE_CODE_COMMITTEE_MEMBER
            .to_vec()
            .try_into()
            .map_err(|_| "PRC fee sweep migration: invalid committee role code")?;
        let module_tag: publicentity::ModuleTagOf = MODULE_MULTISIG
            .to_vec()
            .try_into()
            .map_err(|_| "PRC fee sweep migration: invalid module tag")?;
        let mut updates = Vec::new();

        for institution in CHINA_CB
            .iter()
            .filter(|entry| institution_code_from_cid_number(entry.cid_number) == Some(PRC))
        {
            let cid = publicentity::pallet::CidNumberOf::<Runtime>::try_from(
                institution.cid_number.as_bytes().to_vec(),
            )
            .map_err(|_| "PRC fee sweep migration: invalid institution CID")?;
            let role = publicentity::InstitutionRoles::<Runtime>::get(&cid, &role_code)
                .ok_or("PRC fee sweep migration: fixed committee role missing")?;
            if role.cid_number != cid
                || role.role_code != role_code
                || role.role_status != entitybase::InstitutionRoleStatus::Active
            {
                return Err("PRC fee sweep migration: invalid fixed committee role");
            }

            let mut permissions =
                publicentity::InstitutionRolePermissions::<Runtime>::try_get(&cid, &role_code)
                    .map_err(|_| "PRC fee sweep migration: committee permissions missing")?;
            let old_len = permissions.len();
            for operation in [
                RolePermissionOperation::Propose,
                RolePermissionOperation::Vote,
            ] {
                let permission = entitybase::RoleBusinessPermission {
                    role_subject: entitybase::RoleSubject {
                        cid_number: cid.clone(),
                        role_code: role_code.clone(),
                    },
                    business_action_id: entitybase::BusinessActionId {
                        module_tag: module_tag.clone(),
                        action_code: ACTION_FEE_SWEEP_TO_MAIN,
                    },
                    operation,
                };
                if !permissions.contains(&permission) {
                    permissions.try_push(permission).map_err(|_| {
                        "PRC fee sweep migration: committee permissions at capacity"
                    })?;
                }
            }
            if permissions.len() != old_len {
                updates.push((cid, permissions));
            }
        }
        Ok((role_code, updates))
    }
}

impl OnRuntimeUpgrade for AddPrcFeeSweepPermissions {
    fn on_runtime_upgrade() -> Weight {
        let db = RocksDbWeight::get();
        // 提案编号原地扩宽：复用现有升级入口与 VotingEngine 存储版本，不增模块或存储表。
        // 必须独立于下面的权限迁移标记，否则已经补过权限的节点会漏掉提案转换。
        use codec::{DecodeAll, Encode};
        use frame_support::{storage::{with_transaction, TransactionOutcome}, traits::StorageVersion,
        };
        let mut proposal_weight = db.reads(1);
        if StorageVersion::get::<engingcore::Pallet<Runtime>>() == StorageVersion::new(0) {
            let result: Result<Weight, sp_runtime::DispatchError> = with_transaction(|| {
                let convert = || -> Result<Weight, sp_runtime::DispatchError> {
                    let invalid = || sp_runtime::DispatchError::Other("提案编号旧布局校验失败");
                    let mut updates = Vec::new();
                    let mut bytes = 0u64;
                    // 键：0 原键，1 首维 Blake2，2 首维 Twox，3 末维 Twox，4 年份索引。
                    // 值：0 原值，1 ID，2 ID 列表，3 展示号，4 清理任务，5 互斥锁，
                    // 6 法律版本，7 升级审计，8 GRANDPA 调度，9 年份，10 年内计数。
                    let layouts: &[(&[u8], &[u8], u8, u8)] = &[
                    (b"VotingEngine", b"CurrentProposalYear", 0, 9),
                    (b"VotingEngine", b"YearProposalCounter", 0, 10),
                    (b"VotingEngine", b"NextProposalId", 0, 1),
                    (b"VotingEngine", b"Proposals", 1, 0),
                    (b"VotingEngine", b"CallbackExecutionScopes", 1, 0),
                    (b"VotingEngine", b"ProposalsByExpiry", 0, 2),
                    (b"VotingEngine", b"PendingProposalCleanups", 1, 0),
                    (b"VotingEngine", b"AdminSnapshot", 1, 0),
                    (b"VotingEngine", b"ProposalVotePlans", 1, 0),
                    (b"VotingEngine", b"VoterSnapshot", 1, 0),
                    (b"VotingEngine", b"InstitutionTicketCountSnapshot", 1, 0),
                    (b"VotingEngine", b"ProposalData", 1, 0),
                    (b"VotingEngine", b"ProposalOwner", 1, 0),
                    (b"VotingEngine", b"ProposalPopulationSnapshots", 1, 0),
                    (b"VotingEngine", b"ProposalExecutionRetryStates", 1, 0),
                    (b"VotingEngine", b"PendingProposalExecutions", 1, 0),
                    (b"VotingEngine", b"PendingTerminalFinalizations", 1, 0),
                    (b"VotingEngine", b"TerminalFinalizationDeadLetters", 1, 0),
                    (b"VotingEngine", b"AutoFinalizeRetryStates", 1, 0),
                    (b"VotingEngine", b"AutoFinalizeDeadLetters", 1, 0),
                    (b"VotingEngine", b"PendingExecutionRetryExpirations", 1, 0),
                    (b"VotingEngine", b"PendingTerminalCleanups", 1, 0),
                    (b"VotingEngine", b"ExecutionRetryDeadlines", 0, 2),
                    (b"VotingEngine", b"ProposalObjectMeta", 1, 0),
                    (b"VotingEngine", b"ProposalObject", 1, 0),
                    (b"VotingEngine", b"ProposalMeta", 1, 0),
                    (b"VotingEngine", b"ScheduledCleanups", 0, 4),
                    (b"VotingEngine", b"PendingCleanupQueue", 0, 1),
                    (b"VotingEngine", b"ActiveProposalsBySubject", 0, 2),
                    (b"VotingEngine", b"InternalProposalMutexes", 0, 5),
                    (b"VotingEngine", b"ProposalMutexBindings", 1, 0),
                    (b"VotingEngine", b"ProposalDisplayId", 1, 3),
                    (b"VotingEngine", b"ProposalsByCode", 3, 0),
                    (b"VotingEngine", b"ProposalsByCid", 3, 0),
                    (b"VotingEngine", b"ProposalsByOwner", 3, 0),
                    (b"VotingEngine", b"ProposalsByYear", 4, 0),
                    (b"InternalVote", b"InternalVotesByTicket", 1, 0),
                    (b"InternalVote", b"InternalTallies", 1, 0),
                    (b"InternalVote", b"InternalThresholdSnapshot", 1, 0),
                    (b"InternalVote", b"PendingPersonalThresholds", 1, 0),
                    (b"InternalVote", b"PendingPersonalAdminChangeThresholds", 1, 0,
                        ),
                    (b"InternalVote", b"InternalProposalRoles", 1, 0),
                    (b"JointVote", b"JointVotesByTicket", 1, 0),
                    (b"JointVote", b"JointInstitutionTallies", 1, 0),
                    (b"JointVote", b"JointVotesByInstitution", 1, 0),
                    (b"JointVote", b"JointTallies", 1, 0),
                    (b"JointVote", b"ReferendumVotesByCid", 1, 0),
                    (b"JointVote", b"ReferendumTallies", 1, 0),
                    (b"ElectionVote", b"ElectionMetaStore", 1, 0),
                    (b"ElectionVote", b"ElectionCandidates", 1, 0),
                    (b"ElectionVote", b"PopularElectionVotesByCid", 1, 0),
                    (b"ElectionVote", b"MutualElectionVotesByTicket", 1, 0),
                    (b"ElectionVote", b"ElectionCandidateTallies", 1, 0),
                    (b"ElectionVote", b"ElectionTallyStore", 1, 0),
                    (b"ElectionVote", b"ElectionResults", 1, 0),
                    (b"LegislationVote", b"RepresentativeMetas", 1, 0),
                    (b"LegislationVote", b"LegislationMetas", 1, 0),
                    (b"LegislationVote", b"RepresentativeTallies", 1, 0),
                    (b"LegislationVote", b"RepresentativeVotesByTicket", 1, 0),
                    (b"LegislationVote", b"LegReferendumTally", 1, 0),
                    (b"LegislationVote", b"LegReferendumVotesByCid", 1, 0),
                    (b"LegislationVote", b"LegOverrideSigns", 1, 0),
                    (b"LegislationVote", b"LegGuardSigns", 1, 0),
                    (b"Personal", b"PendingPersonalCreate", 1, 0),
                    (b"Personal", b"PendingCloseProposal", 0, 1),
                    (b"PublicEntity", b"InstitutionPendingClose", 0, 1),
                    (b"PublicEntity", b"InstitutionPendingAdd", 0, 1),
                    (b"PrivateEntity", b"InstitutionPendingClose", 0, 1),
                    (b"PrivateEntity", b"InstitutionPendingAdd", 0, 1),
                    (b"Multisig", b"SafetyFundProposalActions", 1, 0),
                    (b"Multisig", b"SweepProposalActions", 1, 0),
                    (b"ResolutionIssue", b"Executed", 2, 0),
                    (b"ResolutionIssue", b"EverExecuted", 2, 0),
                    (b"GrandpaKeyChange", b"ActiveEmergencyRecoveryByInstitution", 0, 1,
                        ),
                    (b"GrandpaKeyChange", b"PendingGrandpaKeyChange", 0, 8),
                    (b"Upgrade", b"LastRuntimeUpgradeAudit", 0, 7),
                    (b"Legislation", b"LawVersions", 0, 6),
                    ];
                    for &(pallet, item, key_layout, value_layout) in layouts {
                        let prefix = frame_support::storage::storage_prefix(pallet, item);
                        let mut keys = Vec::new();
                        if sp_io::storage::exists(&prefix) { keys.push(prefix.to_vec()); }
                        let mut cursor = prefix.to_vec();
                        while let Some(key) = sp_io::storage::next_key(&cursor) {
                            if !key.starts_with(&prefix) { break; }
                            cursor = key.clone();
                            keys.push(key);
                        }
                        for old_key in keys {
                            let raw = sp_io::storage::get(&old_key).ok_or_else(invalid)?;
                            bytes = bytes.saturating_add((old_key.len() + raw.len()) as u64);
                            let mut key = old_key.clone();
                            if key_layout == 4 {
                                if old_key.len() != 58 { return Err(invalid()); }
                                let year = u16::decode_all(&mut &old_key[40..42]).map_err(|_| invalid())?;
                                let id = u64::decode_all(&mut &old_key[50..58]).map_err(|_| invalid())?;
                                if sp_io::hashing::twox_64(&old_key[40..42]) != old_key[32..40]
                                    || sp_io::hashing::twox_64(&old_key[50..58]) != old_key[42..50] { return Err(invalid()); }
                                key = prefix.to_vec();
                                let year = u32::from(year).encode();
                                let id = u128::from(id).encode();
                                key.extend_from_slice(&sp_io::hashing::twox_64(&year)); key.extend(year);
                                key.extend_from_slice(&sp_io::hashing::twox_64(&id)); key.extend(id);
                            } else if key_layout != 0 {
                                let hash_len = if key_layout == 1 { 16 } else { 8 };
                                let start = if key_layout == 3 { old_key.len().checked_sub(16).ok_or_else(invalid)? } else { 32 };
                                let end = start + hash_len + 8;
                                if old_key.len() < end { return Err(invalid()); }
                                let encoded = &old_key[start + hash_len..end];
                                let id = u64::decode_all(&mut &encoded[..]).map_err(|_| invalid())?;
                                let old_hash = if hash_len == 16 { sp_io::hashing::blake2_128(encoded).to_vec() } else { sp_io::hashing::twox_64(encoded).to_vec() };
                                if old_hash != old_key[start..start + hash_len] { return Err(invalid()); }
                                let encoded = u128::from(id).encode();
                                key = old_key[..start].to_vec();
                                if hash_len == 16 { key.extend_from_slice(&sp_io::hashing::blake2_128(&encoded)); }
                                else { key.extend_from_slice(&sp_io::hashing::twox_64(&encoded)); }
                                key.extend(encoded); key.extend_from_slice(&old_key[end..]);
                            }
                            let mut input = &raw[..];
                            let value = match value_layout {
                                0 => raw.to_vec(),
                                1 => {
                                    u128::from(u64::decode_all(&mut input).map_err(|_| invalid())?).encode()
                                }
                                2 => Vec::<u64>::decode_all(&mut input).map_err(|_| invalid())?.into_iter().map(u128::from).collect::<Vec<_>>().encode(),
                                3 => { let (year, seq) = <(u16, u32)>::decode_all(&mut input).map_err(|_| invalid())?; (u32::from(year), u64::from(seq)).encode() }
                                4 => { let (at, id) = <(BlockNumber, u64)>::decode_all(&mut input).map_err(|_| invalid())?; (at, u128::from(id)).encode() }
                                5 => { let (id, count) = <(Option<u64>, u32)>::decode_all(&mut input).map_err(|_| invalid())?; (id.map(u128::from), count).encode() }
                                6 => {
                                    // 旧 LawVersion 尾部固定为 ID、发布时间、生效时间；正文规范字节不动。
                                    let offset = raw.len().checked_sub(24).ok_or_else(invalid)?;
                                    let id = u64::decode_all(&mut &raw[offset..offset + 8]).map_err(|_| invalid())?;
                                    let mut out = raw[..offset].to_vec(); out.extend(u128::from(id).encode()); out.extend_from_slice(&raw[offset + 8..]);
                                    legislation::pallet::LawVersion::<Runtime>::decode_all(&mut &out[..],
                                    ).map_err(|_| invalid())?; out
                                }
                                7 | 8 => {
                                    let offset = if value_layout == 8 {
                                        <(Vec<u8>, AccountId, [u8; 32], [u8; 32], u64, u8)>::decode(&mut input).map_err(|_| invalid())?;
                                        raw.len() - input.len()
                                    } else { 0 };
                                    let id = Option::<u64>::decode(&mut input).map_err(|_| invalid())?;
                                    let mut out = raw[..offset].to_vec(); out.extend(id.map(u128::from).encode()); out.extend_from_slice(input); out
                                }
                                9 => u32::from(u16::decode_all(&mut input).map_err(|_| invalid())?).encode(),
                                10 => {
                                    u64::from(u32::decode_all(&mut input).map_err(|_| invalid())?).encode()
                                }
                                _ => return Err(invalid()),
                            };
                            if key != old_key && sp_io::storage::exists(&key) { return Err(invalid()); }
                            updates.push((old_key, key, value));
                        }
                    }
                    // 先收集再写入，禁止同前缀边遍历边插入；超预算时不提交任何转换。
                    let rows = updates.len() as u64;
                    let weight = db.reads_writes(rows.saturating_mul(3).saturating_add(layouts.len() as u64 * 2), rows.saturating_mul(2).saturating_add(1),
                        )
                        .saturating_add(Weight::from_parts(1_000_000_000u64.saturating_add(bytes.saturating_mul(10_000)), bytes.saturating_mul(2).saturating_add(rows * 512),
                        ));
                    if weight.any_gt(RuntimeBlockWeights::get().max_block) { return Err(invalid()); }
                    for (old, key, value) in &updates { if old != key { sp_io::storage::clear(old); } sp_io::storage::set(key, value); }
                    StorageVersion::new(1).put::<engingcore::Pallet<Runtime>>();
                    Ok(weight)
                };
                match convert() { Ok(weight) => TransactionOutcome::Commit(Ok(weight)), Err(error) => TransactionOutcome::Rollback(Err(error)),
                }
            });
            // 新 Runtime 不能继续消费未转换的旧布局；失败使本次升级区块无效，禁止静默跳过。
            proposal_weight = proposal_weight.saturating_add(result.expect("提案编号原地转换必须完整成功"));
        }

        // 提案转换仍在原入口执行。权限补齐等到机构键搬完才读取新的类型化存储。
        if crate::migrations::Institutions::pending() {
            return proposal_weight.saturating_add(db.reads(1));
        }
        if frame_support::storage::unhashed::get::<bool>(PRC_FEE_SWEEP_MIGRATED) == Some(true) {
            return proposal_weight.saturating_add(db.reads(1));
        }
        // 每个固定 CID 最多读取岗位和权限各一次，另计迁移标记。
        let reads = 1 + 2 * primitives::cid::china::china_cb::CHINA_CB.len() as u64;
        // 先验证所有目标并准备有界权限列表，再写入，避免只更新部分省储委会。
        let (role_code, updates) = match Self::prepare() {
            Ok(prepared) => prepared,
            Err(error) => {
                sp_runtime::print(error);
                return proposal_weight.saturating_add(db.reads(reads));
            }
        };
        let writes = updates.len() as u64 + 1;
        for (cid, permissions) in updates {
            publicentity::InstitutionRolePermissions::<Runtime>::insert(
                cid,
                &role_code,
                permissions,
            );
        }
        frame_support::storage::unhashed::put(PRC_FEE_SWEEP_MIGRATED, &true);
        proposal_weight.saturating_add(db.reads_writes(reads, writes))
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
        if !crate::migrations::Institutions::legacy() {
            Self::prepare()?;
        }
        Ok(Vec::new())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
        if crate::migrations::Institutions::pending() {
            return Ok(());
        }
        let (_, pending) = Self::prepare()?;
        if !pending.is_empty() {
            return Err("PRC fee sweep migration: permissions remain missing".into());
        }
        Ok(())
    }
}

pub fn is_stake_account(address: &AccountId) -> bool {
    primitives::cid::china::china_ch::CHINA_CH
        .iter()
        .any(|n| address == &AccountId::new(n.stake_account))
}

fn is_reserved_fee_account(address: &AccountId) -> bool {
    primitives::cid::china::china_ch::CHINA_CH
        .iter()
        .any(|n| address == &AccountId::new(n.fee_account))
}

/// 检查是否为国家储委会安全基金账户。
fn is_safety_fund_account(address: &AccountId) -> bool {
    address == &AccountId::new(primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT)
}

/// 检查是否为国家储委会两和基金账户。
fn is_nrc_he_account(address: &AccountId) -> bool {
    address == &AccountId::new(primitives::cid::china::china_cb::NRC_HE_ACCOUNT)
}

/// 检查是否为储委会费用账户（44 个机构的 fee_account）。
fn is_cb_fee_account(address: &AccountId) -> bool {
    primitives::cid::china::china_cb::CHINA_CB
        .iter()
        .any(|n| address == &AccountId::new(n.fee_account))
}

/// 检查是否为公民链技术发展基金会费用账户。
fn is_citizenchain_fee_account(address: &AccountId) -> bool {
    address
        == &AccountId::new(
            primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION.fee_account,
        )
}

fn is_reserved_main_account(address: &AccountId) -> bool {
    let raw: &[u8] = address.as_ref();
    if raw.len() != 32 {
        return false;
    }
    let mut addr = [0u8; 32];
    addr.copy_from_slice(raw);
    primitives::cid::china::china_zb::is_reserved_main_account(&addr)
}

/// 是否为某私法人股份公司（SFGF）的【清算账户】（承载 L2 用户存款准备金）。
///
/// 清算账户是运行期动态注册的私权机构账户，不在任何创世保留表内，故用私法人反查
/// 索引 `privateentity::AccountRegisteredCid` 按 `account_name == "清算账户"` 判定。
/// 清算行已确认是私法人（SFGF），只查 privateentity。
fn is_clearing_account(address: &AccountId) -> bool {
    privateentity::AccountRegisteredCid::<Runtime>::get(address)
        .map(|info| {
            info.account_name.as_slice() == primitives::account_derive::RESERVED_NAME_CLEARING
        })
        .unwrap_or(false)
}

/// 是否为联邦安全局（FSC）的【联邦公民安全基金】账户。
///
/// FSC 是总统府下属公权机构，故查 `publicentity::AccountRegisteredCid` 反查索引，
/// 按 `account_name == "联邦公民安全基金"` 判定。该基金为 FSC 专属，其他机构没有。
fn is_federal_citizen_security_fund_account(address: &AccountId) -> bool {
    publicentity::AccountRegisteredCid::<Runtime>::get(address)
        .map(|info| info.account_name.as_slice() == primitives::account_derive::RESERVED_NAME_FCSF)
        .unwrap_or(false)
}

pub struct RuntimeCallFilter;

impl Contains<RuntimeCall> for RuntimeCallFilter {
    fn contains(call: &RuntimeCall) -> bool {
        // 机构存储尚未搬完时，禁止治理、任职及身份写入改变迁移证据。
        // 保留现有转账和签名备注供非空区块推进有界续跑，不豁免空块规则。
        if crate::migrations::Institutions::pending() {
            return matches!(call,
                RuntimeCall::Timestamp(_)
                    | RuntimeCall::System(frame_system::Call::remark { .. })
                    | RuntimeCall::System(frame_system::Call::remark_with_event { .. })
                    | RuntimeCall::Onchain(onchain::pallet::Call::transfer_with_remark { .. })
            );
        }
        match call {
            // 只开放 Ethereum 外层及验签转换后的执行入口；来源由 SetOrigin 校验。
            // 普通合约不能借原生 dispatch 入口访问业务 pallet。
            RuntimeCall::Revive(call) => {
                !crate::evm_accounts::BackfillEvmAccounts::pending() && matches!(
                call,
                pallet_revive::Call::eth_transact { .. }
                    | pallet_revive::Call::eth_call { .. }
                    | pallet_revive::Call::eth_instantiate_with_code { .. }
            )
            }
            // Balances 只作为底层余额账本和内部 Currency 能力保留。
            // 外部单账户链上转账唯一入口是 Onchain::transfer_with_remark。
            RuntimeCall::Balances(_) => false,
            // ADR-011 铁律:pallet_assets 内核所有原生 extrinsic 一律 reject。
            // 业务调用必须经由 OnchainIssue::propose_* → InternalVote/JointVote callback → 内部 root 调用。
            // 任何外部 extrinsic 直接打到 pallet_assets 全部不入块,
            // 这是用户代币治理唯一入口铁律的链端兜底。
            RuntimeCall::Assets(_) => false,
            // 未启用模块:onchainissue(ADR-011 用户代币,当前为空壳)与
            // offchain-transaction(链下清算行,业务未启用)一律 reject 直接外部调用。
            // offchain-transaction 的开户、充值、提现、换行、批次、费率和节点登记
            // 全部保持整体禁用；任何单个调用均不得因 origin 或参数不同绕过本过滤器。
            // 后续启用必须另行确认完整业务规则、岗位权限和指定投票引擎，并通过
            // runtime 升级删除对应分支；不能只解除过滤就宣称业务可用。
            RuntimeCall::OnchainIssue(_) => false,
            RuntimeCall::Offchain(_) => false,

            // ── 放行:逐 pallet 显式列出,不设 `_` 通配分支 ──
            // 与同文件 `fee_route` 保持同一策略:新增 pallet 会触发编译期
            // non-exhaustive 错误,强制作者显式决定放行还是拒绝,
            // 不会因为默认放行而悄悄把未审查的 extrinsic 暴露给外部。
            // 系统与共识基础。
            RuntimeCall::System(_) | RuntimeCall::Timestamp(_) | RuntimeCall::Grandpa(_) => true,
            // 交易与发行。
            RuntimeCall::Onchain(_)
            | RuntimeCall::Multisig(_)
            | RuntimeCall::FullnodeIssue(_)
            | RuntimeCall::ResolutionIssue(_)
            | RuntimeCall::CitizenIssue(_) => true,
            // 投票引擎核心与四个 sub-pallet。
            RuntimeCall::VotingEngine(_)
            | RuntimeCall::InternalVote(_)
            | RuntimeCall::JointVote(_)
            | RuntimeCall::ElectionVote(_)
            | RuntimeCall::LegislationVote(_) => true,
            // 治理与协议升级。
            RuntimeCall::Upgrade(_)
            | RuntimeCall::Destroy(_)
            | RuntimeCall::GrandpaKey(_)
            | RuntimeCall::Legislation(_) => true,
            // 机构实体生命周期和独立个人多签。
            RuntimeCall::PublicEntity(_)
            | RuntimeCall::PrivateEntity(_)
            | RuntimeCall::Personal(_) => true,
            // 身份与业务索引。
            RuntimeCall::Identity(_)
            | RuntimeCall::Address(_)
            | RuntimeCall::Square(_) => true,
        }
    }
}

/// The default types are being injected by [`derive_impl`](`frame_support::derive_impl`) from
/// [`SoloChainDefaultConfig`](`struct@frame_system::config_preludes::SolochainDefaultConfig`),
/// but overridden as needed.
#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
impl frame_system::Config for Runtime {
    /// The block type for the runtime.
    type Block = Block;
    /// Block & extrinsics weights: base values and limits.
    type BlockWeights = RuntimeBlockWeights;
    /// The maximum length of a block (in bytes).
    type BlockLength = RuntimeBlockLength;
    /// The identifier used to distinguish between accounts.
    type AccountId = AccountId;
    /// The type for storing how many extrinsics an account has signed.
    type Nonce = Nonce;
    /// The type for hashing blocks and tries.
    type Hash = Hash;
    /// Maximum number of block number to block hash mappings to keep (oldest pruned first).
    type BlockHashCount = BlockHashCount;
    /// The weight of database operations that the runtime can invoke.
    type DbWeight = RocksDbWeight;
    /// Version of the runtime.
    type Version = Version;
    /// The data to be stored in an account.
    type AccountData = pallet_balances::AccountData<Balance>;
    /// 地址显示编号（SS58 前缀），统一来自 primitives 制度常量。
    type SS58Prefix = SS58Prefix;
    /// 全局调用过滤器，禁止 stake_account 参与 force_* 余额调用，并封禁强制总发行量调整入口。
    type BaseCallFilter = RuntimeCallFilter;
    // Receive aliases persist even when the System balance account is reaped.
    type OnNewAccount = crate::evm_accounts::EvmAccountMapping;
    type OnKilledAccount = SaveAccountNonce;
    type PostInherents = crate::migrations::EvmBackfill;
    // FRAME 的 OnlyInherents 模式与本链禁止空块冲突；有界迁移由 PostInherents 计费续跑。
    type MultiBlockMigrator = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
    type SingleBlockMigrations = SingleBlockMigrations;
}

impl pallet_timestamp::Config for Runtime {
    /// A timestamp: milliseconds since the unix epoch.
    type Moment = u64;
    // 纯 PoW 共识：时间戳不再依赖 Aura 插槽回调。
    type OnTimestampSet = ();
    // PoW 找到即出块；这里只要求时间戳至少递增 1ms，不用时间戳人为节流。
    type MinimumPeriod = ConstU64<1>;
    type WeightInfo = pallet_timestamp::weights::SubstrateWeight<Runtime>;
}

type AccountData = pallet_balances::AccountData<Balance>;
type ReapedNonces = StorageMap<ReapedNonceStore, Blake2_128Concat, AccountId, Nonce, OptionQuery>;

struct ReapedNonceStore;

impl StorageInstance for ReapedNonceStore {
    fn pallet_prefix() -> &'static str {
        "Runtime"
    }

    const STORAGE_PREFIX: &'static str = "ReapedNonces";
}

fn register_nonce_weight(weight: Weight) {
    if System::block_number() != 0 {
        System::register_extra_weight_unchecked(
            weight.saturating_add(RocksDbWeight::get().reads(1)),
            DispatchClass::Mandatory,
        );
    }
}

/// FRAME 回收记录前留下已用账户的 nonce。零余额期间只保存该序号。
pub struct SaveAccountNonce;

impl OnKilledAccount<AccountId> for SaveAccountNonce {
    fn on_killed_account(who: &AccountId) {
        let nonce = System::account_nonce(who);
        let db = RocksDbWeight::get();
        let mut weight = db.reads(1);
        if nonce != 0 {
            ReapedNonces::mutate(who, |stored| {
                *stored = Some(stored.unwrap_or_default().max(nonce));
            });
            weight = weight.saturating_add(
                Weight::from_parts(2_000_000, 3_593).saturating_add(db.reads_writes(1, 1)),
            );
        }
        register_nonce_weight(weight);
    }
}

/// 入金路径还原已回收账号的 nonce，随后由原 System 账户记录继续维护。
pub struct RuntimeBalanceAccounts;

impl StoredMap<AccountId, AccountData> for RuntimeBalanceAccounts {
    fn get(who: &AccountId) -> AccountData {
        <System as StoredMap<AccountId, AccountData>>::get(who)
    }

    fn try_mutate_exists<R, E: From<sp_runtime::DispatchError>>(
        who: &AccountId,
        mutate: impl FnOnce(&mut Option<AccountData>) -> Result<R, E>,
    ) -> Result<R, E> {
        <System as StoredMap<AccountId, AccountData>>::try_mutate_exists(who, |data| {
            let funded = data.as_ref().is_some_and(|account| account.free != 0);
            let result = mutate(data)?;
            if !funded && data.as_ref().is_some_and(|account| account.free != 0) {
                let db = RocksDbWeight::get();
                let mut weight = Weight::from_parts(2_000_000, 3_593).saturating_add(db.reads(1));
                // Balances 已建立正常提供者引用，System 随后只更新余额字段。
                if let Some(nonce) = ReapedNonces::take(who) {
                    frame_system::Account::<Runtime>::mutate(who, |account| {
                        account.nonce = account.nonce.max(nonce);
                    });
                    weight = weight.saturating_add(db.reads_writes(1, 2));
                }
                register_nonce_weight(weight);
            }
            Ok(result)
        })
    }
}

impl pallet_balances::Config for Runtime {
    type MaxLocks = ConstU32<50>;
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    /// The type for recording an account's balance.
    type Balance = Balance;
    /// The ubiquitous event type.
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = RuntimeDustHandler;
    type ExistentialDeposit = ConstU128<EXISTENTIAL_DEPOSIT>;
    type AccountStore = RuntimeBalanceAccounts;
    type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
    type FreezeIdentifier = RuntimeFreezeReason;
    type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type DoneSlashHandler = ();
}

parameter_types! {
    pub const MaxGrandpaAuthorities: u32 = 64;
    pub const MaxGrandpaNominators: u32 = 0;
    // 保留最近若干 set_id 与会话映射，便于后续接入等值投票追溯/举报能力。
    pub const MaxSetIdSessionEntries: u64 = 128;
}

impl pallet_grandpa::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    type MaxAuthorities = MaxGrandpaAuthorities;
    type MaxNominators = MaxGrandpaNominators;
    type MaxSetIdSessionEntries = MaxSetIdSessionEntries;
    // 当前版本不启用链上等值投票惩罚（无 session/historical 证明体系）。
    // 但保留 MaxSetIdSessionEntries 以便后续平滑接入。
    type KeyOwnerProof = Void;
    type EquivocationReportSystem = ();
}

parameter_types! {
    pub FeeMultiplier: Multiplier = Multiplier::one();
}

impl pallet_transaction_payment::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction = onchain::OnchainChargeAdapter<
        Balances,
        onchain::OnchainFeeRouter<
            Runtime,
            Balances,
            PowDigestAuthor,
            RuntimeNrcAccountProvider,
            RuntimeSafetyFundAccountProvider,
        >,
        RuntimeFeeRouter,
    >;
    type OperationalFeeMultiplier = ConstU8<{ primitives::fee_policy::OPERATIONAL_FEE_MULTIPLIER }>;
    type WeightToFee = ConstantMultiplier<Balance, ConstU128<0>>;
    type LengthToFee = ConstantMultiplier<Balance, ConstU128<0>>;
    type FeeMultiplierUpdate = ConstFeeMultiplier<FeeMultiplier>;
    type WeightInfo = crate::evm_accounts::EvmPaymentWeight;
}

parameter_types! {
    /// 链上交易费率下发到 metadata 的转发值；真源恒为 `primitives::fee_policy`。
    pub const RuntimeOnchainFeeRate: Perbill = primitives::fee_policy::ONCHAIN_FEE_RATE;
}

impl onchain::pallet::Config for Runtime {
    type WeightInfo = crate::evm_accounts::EvmOnchainWeight;
    type Currency = Balances;
    type MaxTransferRemarkLen = ConstU32<99>;
    // 三项收费参数只做转发，不在此处另立数字；客户端从 metadata 读取后自行预估费用。
    type OnchainMinFee = ConstU128<{ primitives::fee_policy::ONCHAIN_MIN_FEE }>;
    type OnchainFeeRate = RuntimeOnchainFeeRate;
    type FLAT_FEE = ConstU128<{ primitives::fee_policy::FLAT_FEE }>;
}

pub struct RuntimeNrcAccountProvider;

impl onchain::NrcAccountProvider<AccountId> for RuntimeNrcAccountProvider {
    fn nrc_account() -> Option<AccountId> {
        Some(AccountId::new(
            primitives::cid::china::china_cb::CHINA_CB[0].fee_account,
        ))
    }
}

pub struct RuntimeSafetyFundAccountProvider;

impl onchain::SafetyFundAccountProvider<AccountId> for RuntimeSafetyFundAccountProvider {
    fn safety_fund_account() -> AccountId {
        AccountId::new(primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT)
    }
}

pub struct RuntimeDustHandler;

impl OnUnbalanced<Credit<AccountId, Balances>> for RuntimeDustHandler {
    fn on_nonzero_unbalanced(amount: Credit<AccountId, Balances>) {
        if let Some(nrc_account) = RuntimeNrcAccountProvider::nrc_account() {
            if let Err(remaining) = Balances::resolve(&nrc_account, amount) {
                drop(remaining);
            }
        } else {
            drop(amount);
        }
    }
}

pub struct RuntimeFeeRouter;

fn signer_onchain_route(
    who: &AccountId,
    transaction_amount: Balance,
) -> primitives::fee_policy::FeeRoute<AccountId, Balance> {
    primitives::fee_policy::FeeRoute::Onchain {
        transaction_amount,
        payer_account_id: who.clone(),
    }
}

fn signer_vote_route(who: &AccountId) -> primitives::fee_policy::FeeRoute<AccountId, Balance> {
    primitives::fee_policy::FeeRoute::Vote {
        payer_account_id: who.clone(),
    }
}

/// 严格读取 `(cid_number, 费用账户)`；公权/私权重复、正反索引不一致或账户缺失均失败。
fn exact_institution_fee_account(cid_number: &[u8]) -> Option<AccountId> {
    RuntimeInstitutionQuery::lookup_institution_account(
        cid_number,
        primitives::account_derive::RESERVED_NAME_FEE,
    )
}

/// 账户型机构交易必须显式携带同一 CID 下的具体机构账户，禁止由账户反推或跨 CID 使用。
fn exact_institution_account_matches(cid_number: &[u8], account: &AccountId) -> bool {
    RuntimeInstitutionQuery::account_belongs_to(cid_number, account)
}

/// 机构代付只依据本次调用声明的有效岗位，避免遍历机构的全部岗位任职。
fn is_authorized_institution_actor(who: &AccountId, cid_number: &[u8], role_code: &[u8]) -> bool {
    match institution_role_directory(cid_number) {
        Some(true) => PublicEntity::is_active_assignment(cid_number, who, role_code),
        Some(false) => PrivateEntity::is_active_assignment(cid_number, who, role_code),
        None => false,
    }
}

fn institution_charge(cid_number: &[u8]) -> primitives::fee_policy::FeeRoute<AccountId, Balance> {
    match exact_institution_fee_account(cid_number) {
        Some(payer_account_id) => primitives::fee_policy::FeeRoute::Onchain {
            transaction_amount: 0,
            payer_account_id,
        },
        None => primitives::fee_policy::FeeRoute::Reject,
    }
}

fn institution_onchain_route(
    who: &AccountId, cid_number: &[u8], role_code: &[u8],
) -> primitives::fee_policy::FeeRoute<AccountId, Balance> {
    if is_authorized_institution_actor(who, cid_number, role_code) {
        institution_charge(cid_number)
    } else {
        primitives::fee_policy::FeeRoute::Reject
    }
}

fn institution_account_onchain_route(
    who: &AccountId, cid_number: &[u8], role_code: &[u8], institution_account_id: &AccountId,
) -> primitives::fee_policy::FeeRoute<AccountId, Balance> {
    if !exact_institution_account_matches(cid_number, institution_account_id) {
        return primitives::fee_policy::FeeRoute::Reject;
    }
    institution_onchain_route(who, cid_number, role_code)
}

fn proposal_operation_route(
    who: &AccountId, proposal_id: u128,
) -> primitives::fee_policy::FeeRoute<AccountId, Balance> {
    use primitives::fee_policy::FeeRoute;
    let Some(proposal) = engingcore::Pallet::<Runtime>::proposals(proposal_id) else {
        return FeeRoute::Reject;
    };
    let Some(cid) = proposal.actor_cid_number else {
        return signer_onchain_route(who, 0);
    };
    let Some(plan) = engingcore::Pallet::<Runtime>::proposal_vote_plan(proposal_id) else {
        return FeeRoute::Reject;
    };
    // 重试和取消仍由发起机构的费用账户代付，签名人须持有该机构仍有效的冻结岗位。
    let eligible = plan.voter_subjects.into_iter().any(|subject| {
        let entitybase::AuthorizationSubject::Institution(role) = &subject else {
            return false;
        };
        role.cid_number == cid
            && is_authorized_institution_actor(who, cid.as_slice(), role.role_code.as_slice())
            && engingcore::Pallet::<Runtime>::is_subject_voter_in_snapshot(proposal_id, subject, who)
    });
    if eligible {
        institution_charge(&cid)
    } else {
        FeeRoute::Reject
    }
}

impl onchain::CallFeeRoute<AccountId, RuntimeCall, Balance> for RuntimeFeeRouter {
    fn fee_route(
        who: &AccountId,
        call: &RuntimeCall,
    ) -> primitives::fee_policy::FeeRoute<AccountId, Balance> {
        use primitives::fee_policy::FeeRoute;

        match call {
            RuntimeCall::Revive(_) if crate::evm_accounts::BackfillEvmAccounts::pending() => {
                FeeRoute::Reject
            }
            // 金额只做严格整分转换，费用分类和计算仍走既有 signer_onchain_route。
            RuntimeCall::Revive(
                pallet_revive::Call::eth_call { value, .. }
                | pallet_revive::Call::eth_instantiate_with_code { value, .. },
            ) => match pallet_revive::BalanceWithDust::<Balance>::from_value::<Runtime>(*value) {
                Ok(value) => signer_onchain_route(who, value.deconstruct().0),
                Err(_) => FeeRoute::Reject,
            },
            RuntimeCall::Revive(_) => FeeRoute::Reject,
            RuntimeCall::Onchain(onchain::pallet::Call::transfer_with_remark {
                amount,
                ..
            }) => signer_onchain_route(who, *amount),

            // 个人多签不是机构；创建提案和管理员变更属于普通链上操作，只有 cast 才是投票。
            RuntimeCall::Personal(personal::pallet::Call::propose_create {
                ..
            })
            | RuntimeCall::Personal(personal::pallet::Call::propose_close { .. })
            | RuntimeCall::Personal(
                personal::pallet::Call::propose_admin_set_change { .. },
            ) => signer_onchain_route(who, 0),

            // 注册局任职人员用现绑钱包签名，交易费由本次行权机构的费用账户支付。
            RuntimeCall::PublicEntity(
                publicentity::pallet::Call::update_institution_info { actor_cid_number, actor_role_code, .. }
                | publicentity::pallet::Call::register_institution_roles { actor_cid_number, actor_role_code, .. },
            ) | RuntimeCall::PrivateEntity(
                privateentity::pallet::Call::update_institution_info { actor_cid_number, actor_role_code, .. }
                | privateentity::pallet::Call::register_institution_roles { actor_cid_number, actor_role_code, .. },
            ) => institution_onchain_route(who, actor_cid_number, actor_role_code),
            RuntimeCall::PublicEntity(
                publicentity::pallet::Call::propose_institution_governance { actor_cid_number, proposer_role_code, .. },
            ) | RuntimeCall::PrivateEntity(
                privateentity::pallet::Call::propose_institution_governance { actor_cid_number, proposer_role_code, .. },
            ) => institution_onchain_route(who, actor_cid_number, proposer_role_code),
            RuntimeCall::PublicEntity(
                publicentity::pallet::Call::propose_add_institution_account { cid_number, proposer_role_code, .. },
            ) | RuntimeCall::PrivateEntity(
                privateentity::pallet::Call::propose_add_institution_account { cid_number, proposer_role_code, .. },
            ) => institution_onchain_route(who, cid_number, proposer_role_code),
            RuntimeCall::PublicEntity(
                publicentity::pallet::Call::propose_close_public_institution { actor_cid_number, institution_account_id, proposer_role_code, .. },
            ) | RuntimeCall::PrivateEntity(
                privateentity::pallet::Call::propose_close_private_institution { actor_cid_number, institution_account_id, proposer_role_code, .. },
            ) => institution_account_onchain_route(who, actor_cid_number, proposer_role_code, institution_account_id),

            RuntimeCall::Address(
                address::pallet::Call::set_catalog_version {
                    actor_cid_number, actor_role_code, ..
                }
                | address::pallet::Call::set_address_name {
                    actor_cid_number, actor_role_code, ..
                }
                | address::pallet::Call::remove_address_name {
                    actor_cid_number, actor_role_code, ..
                }
                | address::pallet::Call::set_address {
                    actor_cid_number, actor_role_code, ..
                }
                | address::pallet::Call::remove_address {
                    actor_cid_number, actor_role_code, ..
                },
            ) => institution_onchain_route(who, actor_cid_number.as_slice(), actor_role_code.as_slice()),

            // 框架固有、共识公益和 Root/内部维护调用免费。
            RuntimeCall::System(_)
            | RuntimeCall::Timestamp(_)
            | RuntimeCall::CitizenIssue(_)
            | RuntimeCall::Grandpa(_) => FeeRoute::Free,
            RuntimeCall::ResolutionIssue(resolutionissue::pallet::Call::propose_issuance {
                actor_cid_number, proposer_role_code,
                ..
            }) => institution_onchain_route(who, actor_cid_number.as_slice(), proposer_role_code.as_slice()),
            RuntimeCall::Destroy(destroy::pallet::Call::propose_destroy {
                actor_cid_number, proposer_role_code,
                institution_account_id,
                ..
            }) => institution_account_onchain_route(who, actor_cid_number.as_slice(), proposer_role_code.as_slice(), institution_account_id),

            RuntimeCall::VotingEngine(engingcore::pallet::Call::finalize_proposal { .. }) => {
                FeeRoute::Free
            }
            RuntimeCall::VotingEngine(
                engingcore::pallet::Call::retry_passed_proposal { proposal_id }
                | engingcore::pallet::Call::cancel_passed_proposal { proposal_id, .. },
            ) => proposal_operation_route(who, *proposal_id),

            RuntimeCall::Identity(
                identity::pallet::Call::self_occupy_cid { .. }
                | identity::pallet::Call::self_rebind_cid_account_id { .. },
            ) => signer_onchain_route(who, 0),

            RuntimeCall::Identity(
                identity::pallet::Call::register_voting_identity {
                    actor_cid_number, actor_role_code, ..
                }
                | identity::pallet::Call::upgrade_to_candidate_identity {
                    actor_cid_number, actor_role_code, ..
                }
                | identity::pallet::Call::update_voting_identity {
                    actor_cid_number, actor_role_code, ..
                }
                | identity::pallet::Call::update_candidate_identity {
                    actor_cid_number, actor_role_code, ..
                }
                | identity::pallet::Call::revoke_identity {
                    actor_cid_number, actor_role_code, ..
                }
                | identity::pallet::Call::occupy_cid {
                    actor_cid_number, actor_role_code, ..
                }
                | identity::pallet::Call::registrar_rebind_cid {
                    actor_cid_number, actor_role_code, ..
                }
                | identity::pallet::Call::revoke_cid {
                    actor_cid_number, actor_role_code, ..
                },
            ) => institution_onchain_route(who, actor_cid_number.as_slice(), actor_role_code.as_slice()),

            // 广场内容域：只有发帖、订阅、取消、换档和创作者档位管理属于签名动作；
            // 到期续费由 runtime 内部时间戳调度执行，不进入外部 call 路由。
            RuntimeCall::Square(
                square::pallet::Call::publish_post { .. }
                | square::pallet::Call::subscribe { .. }
                | square::pallet::Call::cancel { .. }
                | square::pallet::Call::set_creator_plans { .. }
                | square::pallet::Call::change_subscription_plan { .. },
            )
            | RuntimeCall::FullnodeIssue(
                fullnodeissue::pallet::Call::bind_reward_account { .. }
                | fullnodeissue::pallet::Call::rebind_reward_account { .. },
            ) => signer_onchain_route(who, 0),
            RuntimeCall::Square(square::pallet::Call::propose_set_platform_price {
                actor_cid_number, proposer_role_code,
                ..
            }) => institution_onchain_route(who, actor_cid_number.as_slice(), proposer_role_code.as_slice()),

            RuntimeCall::Upgrade(
                upgrade::pallet::Call::propose_runtime_upgrade {
                    actor_cid_number, actor_role_code, ..
                }
                | upgrade::pallet::Call::developer_direct_upgrade {
                    actor_cid_number, actor_role_code, ..
                },
            ) => institution_onchain_route(who, actor_cid_number.as_slice(), actor_role_code.as_slice()),
            RuntimeCall::GrandpaKey(
                grandpakey::pallet::Call::propose_emergency_grandpa_key_recovery {
                    actor_cid_number, actor_role_code,
                    ..
                }
                | grandpakey::pallet::Call::schedule_grandpa_key_rotation {
                    actor_cid_number, actor_role_code, ..
                },
            ) => institution_onchain_route(who, actor_cid_number.as_slice(), actor_role_code.as_slice()),
            RuntimeCall::Legislation(
                legislation::pallet::Call::propose_enact_law {
                    actor_cid_number, proposer_role_code, ..
                }
                | legislation::pallet::Call::propose_amend_law {
                    actor_cid_number, proposer_role_code, ..
                }
                | legislation::pallet::Call::propose_repeal_law {
                    actor_cid_number, proposer_role_code, ..
                },
            ) => institution_onchain_route(who, actor_cid_number.as_slice(), proposer_role_code.as_slice()),

            RuntimeCall::Multisig(multisig::pallet::Call::propose_transfer {
                actor_cid_number, proposer_role_code,
                funding_account_id,
                ..
            }) => match actor_cid_number {
                Some(cid_number) => match proposer_role_code.as_ref() {
                    Some(role) => institution_account_onchain_route(who, cid_number, role, funding_account_id),
                    None => FeeRoute::Reject,
                },
                None => signer_onchain_route(who, 0),
            },
            RuntimeCall::Multisig(
                multisig::pallet::Call::propose_safety_fund_transfer {
                    actor_cid_number, proposer_role_code,
                    institution_account_id,
                    ..
                }
                | multisig::pallet::Call::propose_sweep_to_main {
                    actor_cid_number, proposer_role_code,
                    institution_account_id,
                    ..
                },
            ) => institution_account_onchain_route(who, actor_cid_number.as_slice(), proposer_role_code.as_slice(), institution_account_id),

            RuntimeCall::Offchain(offchain::pallet::Call::bind_clearing_bank {
                ..
            })
            | RuntimeCall::Offchain(offchain::pallet::Call::switch_bank { .. }) => {
                signer_onchain_route(who, 0)
            }
            RuntimeCall::Offchain(offchain::pallet::Call::deposit { amount })
            | RuntimeCall::Offchain(offchain::pallet::Call::withdraw { amount }) => {
                signer_onchain_route(who, *amount)
            }
            RuntimeCall::Offchain(offchain::pallet::Call::submit_offchain_batch {
                actor_cid_number, actor_role_code,
                institution_account_id,
                batch,
                ..
            }) => {
                if !exact_institution_account_matches(
                    actor_cid_number.as_slice(),
                    institution_account_id,
                ) {
                    return FeeRoute::Reject;
                }
                // 链下费用由各 item 的付款公民承担；这里仍须验证提交机构管理员，
                // 并保证作为手续费收款方的机构费用账户唯一且正反索引一致。
                if !is_authorized_institution_actor(who, actor_cid_number, actor_role_code)
                    || exact_institution_fee_account(actor_cid_number.as_slice()).is_none()
                {
                    return FeeRoute::Reject;
                }
                let fee_amount = batch
                    .iter()
                    .fold(0u128, |sum, item| sum.saturating_add(item.fee_amount));
                FeeRoute::Offchain {
                    fee_amount,
                    payer_account_id: primitives::fee_policy::OffchainFeePayer::BatchItemPayers,
                }
            }
            RuntimeCall::Offchain(offchain::pallet::Call::propose_l2_fee_rate {
                actor_cid_number, actor_role_code,
                institution_account_id,
                ..
            }) => institution_account_onchain_route(who, actor_cid_number.as_slice(), actor_role_code.as_slice(), institution_account_id),
            RuntimeCall::Offchain(
                offchain::pallet::Call::register_clearing_bank {
                    actor_cid_number, actor_role_code, ..
                }
                | offchain::pallet::Call::update_clearing_bank_endpoint {
                    actor_cid_number, actor_role_code, ..
                }
                | offchain::pallet::Call::unregister_clearing_bank {
                    actor_cid_number, actor_role_code, ..
                },
            ) => institution_onchain_route(who, actor_cid_number.as_slice(), actor_role_code.as_slice()),
            RuntimeCall::Offchain(offchain::pallet::Call::set_max_l2_fee_rate {
                ..
            }) => FeeRoute::Free,

            // 只有实际投票/表决动作支付固定 1 元，并且始终由投票签名者本人支付。
            RuntimeCall::InternalVote(internalvote::pallet::Call::cast { .. })
            | RuntimeCall::JointVote(jointvote::pallet::Call::cast_role_vote { .. })
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
            ) => signer_vote_route(who),
            // onchainissue 当前 10 个公开 call 都是明确的业务占位，授权后直接
            // `Ok(())`，尚未创建投票或执行资产逻辑。未实装前统一拒绝，禁止形成
            // “扣了机构操作费但没有业务结果”的假交易。
            RuntimeCall::OnchainIssue(_) => FeeRoute::Reject,

            // FRAME call enum 为元数据稳定性生成 `__Ignore` 隐藏分支；每个业务 pallet
            // 仅把未显式列出的内部 call 归为 Reject。外层 RuntimeCall 不设通配分支，
            // 因此新增 pallet 仍会触发编译期 non-exhaustive 错误。
            RuntimeCall::Onchain(_)
            | RuntimeCall::FullnodeIssue(_)
            | RuntimeCall::ResolutionIssue(_)
            | RuntimeCall::VotingEngine(_)
            | RuntimeCall::InternalVote(_)
            | RuntimeCall::JointVote(_)
            | RuntimeCall::ElectionVote(_)
            | RuntimeCall::Identity(_)
            | RuntimeCall::Upgrade(_)
            | RuntimeCall::Destroy(_)
            | RuntimeCall::GrandpaKey(_)
            | RuntimeCall::Personal(_)
            | RuntimeCall::Multisig(_)
            | RuntimeCall::Offchain(_)
            | RuntimeCall::Legislation(_)
            | RuntimeCall::LegislationVote(_)
            | RuntimeCall::PublicEntity(_)
            | RuntimeCall::PrivateEntity(_)
            | RuntimeCall::Address(_)
            | RuntimeCall::Square(_) => FeeRoute::Reject,

            // 两个内核 pallet 的外部入口被 BaseCallFilter 禁用；显式 Reject，不伪装成免费。
            RuntimeCall::Assets(_) | RuntimeCall::Balances(_) => FeeRoute::Reject,
        }
    }
}

/// 省储行利息模块配置：
/// - 使用 Balances 作为铸币/记账货币
/// - 每年区块数统一采用 primitives 中的制度常量
impl prbbankissue::Config for Runtime {
    type Currency = Balances;
    type BlocksPerYear = ConstU64<{ primitives::pow_const::BLOCKS_PER_YEAR }>;
    type WeightInfo = crate::evm_accounts::EvmProvincialWeight;
}

/// PoW 作者解析器：
/// 从区块 pre-runtime digest 中读取 POW_ENGINE_ID 的负载（sr25519 公钥），
/// 派生为 AccountId。配合 seal 中的签名实现矿工身份密码学绑定。
pub struct PowDigestAuthor;

impl FindAuthor<AccountId> for PowDigestAuthor {
    fn find_author<'a, I>(digests: I) -> Option<AccountId>
    where
        I: 'a + IntoIterator<Item = (sp_runtime::ConsensusEngineId, &'a [u8])>,
    {
        digests.into_iter().find_map(|(engine_id, data)| {
            if engine_id == sp_consensus_pow::POW_ENGINE_ID {
                sp_core::sr25519::Public::decode(&mut &data[..])
                    .ok()
                    .map(|public| {
                        use sp_runtime::traits::IdentifyAccount;
                        sp_runtime::MultiSigner::from(public).into_account()
                    })
            } else {
                None
            }
        })
    }
}

/// 全节点发行模块配置：
/// - 链上货币使用 Balances
/// - 作者识别完全基于 PoW digest（不依赖 Aura/Grandpa）
impl fullnodeissue::Config for Runtime {
    type Currency = Balances;
    type FindAuthor = PowDigestAuthor;
    type WeightInfo = crate::evm_accounts::EvmFullnodeWeight;
}

pub struct RuntimeAccountValidator;

impl primitives::multisig::AccountValidator<AccountId> for RuntimeAccountValidator {
    fn is_valid(account: &AccountId) -> bool {
        // 禁止零账户。
        if account == &AccountId::new([0u8; 32]) {
            return false;
        }

        // 禁止占用“国家储委会/省储委会”的制度保留交易账户。
        if primitives::cid::china::china_cb::CHINA_CB
            .iter()
            .any(|n| account == &AccountId::new(n.main_account))
        {
            return false;
        }

        // 禁止占用“省储行”的制度保留交易账户。
        if primitives::cid::china::china_ch::CHINA_CH
            .iter()
            .any(|n| account == &AccountId::new(n.main_account))
        {
            return false;
        }

        // 禁止占用省储行费用账户（BLAKE2-256 派生）。
        if primitives::cid::china::china_ch::CHINA_CH
            .iter()
            .any(|n| account == &AccountId::new(n.fee_account))
        {
            return false;
        }

        // 禁止占用国家储委会安全基金账户。
        if is_safety_fund_account(account) {
            return false;
        }

        // 禁止占用国家储委会两和基金账户。
        if is_nrc_he_account(account) {
            return false;
        }

        // 禁止占用储委会费用账户（44 个机构）。
        if is_cb_fee_account(account) {
            return false;
        }

        true
    }
}

pub struct RuntimeReservedAccountGuard;
pub struct RuntimeRegistryAuthority;

pub struct RuntimeProtectedSourceChecker;
pub struct RuntimeInstitutionAsset;

impl primitives::multisig::ProtectedSourceChecker<AccountId> for RuntimeProtectedSourceChecker {
    fn is_protected(address: &AccountId) -> bool {
        is_stake_account(address)
    }
}

impl primitives::institution_asset::InstitutionAsset<AccountId> for RuntimeInstitutionAsset {
    fn can_spend(
        source: &AccountId,
        action: primitives::institution_asset::InstitutionAssetAction,
    ) -> bool {
        // 匹配顺序很重要——更具体的账户类型必须放在更宽泛的类型之前。
        // fee_account 同时出现在 CHINA_RESERVED_MAIN_ACCOUNTS 列表中（同由 BLAKE2 派生且统一保留），
        // 如果 is_reserved_main_account 先匹配，fee_account 会被错误地按主账户规则放行。

        // 1. 无私钥系统账户：全禁
        if is_stake_account(source) {
            return false;
        }

        // 2. 省储行费用账户（最具体）：只允许手续费归集
        if is_reserved_fee_account(source) {
            return matches!(
                action,
                primitives::institution_asset::InstitutionAssetAction::OffchainFeeSweepExecute
            );
        }

        // 3. 储委会费用账户（44 个机构）：只允许手续费归集
        if is_cb_fee_account(source) {
            return matches!(
                action,
                primitives::institution_asset::InstitutionAssetAction::OffchainFeeSweepExecute
            );
        }

        // 4. 公民链基金会费用账户：同样只允许手续费归集；必须先于宽泛保留账户匹配。
        if is_citizenchain_fee_account(source) {
            return matches!(
                action,
                primitives::institution_asset::InstitutionAssetAction::OffchainFeeSweepExecute
            );
        }

        // 5. 国家储委会安全基金账户：只允许安全基金转账
        if source == &AccountId::new(primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT) {
            return matches!(
                action,
                primitives::institution_asset::InstitutionAssetAction::NrcSafetyFundTransfer
            );
        }

        // 6. 多签保留主账户：可为机构创建提供本金，也可执行多签转账和关闭。
        if is_reserved_main_account(source) {
            return matches!(
                action,
                primitives::institution_asset::InstitutionAssetAction::InstitutionCreateFunding
                    | primitives::institution_asset::InstitutionAssetAction::MultisigTransferExecute
                    | primitives::institution_asset::InstitutionAssetAction::MultisigCloseExecute
            );
        }

        // 7. 清算行【清算账户】：装用户 L2 存款准备金。只准扫码清算扣款与用户提现；
        //    管理员经 multisig-transfer / 关闭 / 机构注资一律拒绝，堵死挪用池子。
        //    清算行主账户不命中此谓词，仍走普通账户放行（银行自有营运资金照常可动）。
        if is_clearing_account(source) {
            return matches!(
                action,
                primitives::institution_asset::InstitutionAssetAction::L2ClearingDebit
                    | primitives::institution_asset::InstitutionAssetAction::L3WithdrawOut
            );
        }

        // 8. 联邦公民安全基金（FSC 专属）：只允许经 FSC 内部投票的多签转账支出。
        //    该基金由联邦安全局的 LR + 局长两岗经内部投票（2/2 严格过半）复用通用多签
        //    转账动作从中转出；其余资金动作（注资/关闭/费用归集等）一律拒绝，杜绝旁路挪用。
        if let Some(account) = publicentity::AccountRegisteredCid::<Runtime>::get(source) {
            if account.account_name.as_slice() == primitives::account_derive::RESERVED_NAME_FCSF {
                return matches!(
                    action,
                    primitives::institution_asset::InstitutionAssetAction::MultisigTransferExecute
                );
            }
            // 复用同一次账户反查，仅市注册局主账户需要读达标记录。
            if publicentity::creg::main_cid::<Runtime>(&account)
                .is_some_and(|cid| {
                identity::CityRegistryProgress::<Runtime>::get(cid).rank.is_none()
            })
            {
                return false;
            }
        }

        // 9. 普通账户：全放行
        true
    }
}

impl primitives::multisig::ReservedAccountGuard<AccountId> for RuntimeReservedAccountGuard {
    fn is_reserved(account: &AccountId) -> bool {
        // 禁止占用省储行 stake_account（制度保留账户）。
        if primitives::cid::china::china_ch::CHINA_CH
            .iter()
            .any(|n| account == &AccountId::new(n.stake_account))
        {
            return true;
        }

        // 禁止占用省储行费用账户（BLAKE2-256 派生）。
        if primitives::cid::china::china_ch::CHINA_CH
            .iter()
            .any(|n| account == &AccountId::new(n.fee_account))
        {
            return true;
        }

        // 禁止占用国家储委会安全基金账户。
        if is_safety_fund_account(account) {
            return true;
        }

        // 禁止占用国家储委会两和基金账户。
        if is_nrc_he_account(account) {
            return true;
        }

        // 禁止占用储委会费用账户（44 个机构）。
        if is_cb_fee_account(account) {
            return true;
        }

        // 禁止占用任一 SFGF 清算账户地址（承载 L2 用户存款准备金）。
        // 按名注册已由 account_derive::is_forbidden_account_name 挡下，此处补地址级保护。
        if is_clearing_account(account) {
            return true;
        }

        // 禁止占用联邦安全局的联邦公民安全基金账户地址。
        if is_federal_citizen_security_fund_account(account) {
            return true;
        }

        is_reserved_main_account(account)
    }
}

fn cid_institution_code(cid_number: &[u8]) -> Option<primitives::cid::code::InstitutionCode> {
    let text = core::str::from_utf8(cid_number).ok()?;
    primitives::cid::code::institution_code_from_cid_number(text.trim())
}

impl entitybase::RegistryAuthority<AccountId> for RuntimeRegistryAuthority {
    fn can_register_institution_origin(
        registrar: &AccountId,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        target_cid_number: &[u8],
        target_institution_code: primitives::cid::code::InstitutionCode,
    ) -> bool {
        let role_subject = entitybase::RoleSubject {
            cid_number: actor_cid_number.to_vec(),
            role_code: actor_role_code.to_vec(),
        };
        if !RuntimeInstitutionRoleAuthorization::is_authorized(
            registrar,
            &role_subject,
            &entitybase::BusinessActionId {
                module_tag: entitybase::business_action::MODULE_INSTITUTION_REGISTRATION.to_vec(),
                action_code: entitybase::business_action::ACTION_REGISTER_INSTITUTION,
            },
            RolePermissionOperation::Propose,
        ) {
            return false;
        }
        primitives::institution_constraints::registry_role_in_scope(
            actor_cid_number,
            actor_role_code,
            target_cid_number,
            target_institution_code,
        )
    }
}

pub struct RuntimeAddressAuthority;

impl address::AddressUpdateAuthority<AccountId> for RuntimeAddressAuthority {
    fn can_update_catalog(
        who: &AccountId,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        action_code: u32,
    ) -> bool {
        let Ok(actor_text) = core::str::from_utf8(actor_cid_number) else {
            return false;
        };
        if primitives::cid::code::institution_code_from_cid_number(actor_text)
            != Some(primitives::cid::code::FRG)
        {
            return false;
        }
        let role_subject = entitybase::RoleSubject {
            cid_number: actor_cid_number.to_vec(),
            role_code: actor_role_code.to_vec(),
        };
        RuntimeInstitutionRoleAuthorization::is_authorized(
            who,
            &role_subject,
            &entitybase::BusinessActionId {
                module_tag: entitybase::business_action::MODULE_ADDRESS_REGISTRY.to_vec(),
                action_code,
            },
            RolePermissionOperation::Propose,
        )
    }

    fn can_update_address(
        who: &AccountId,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        province_code: &[u8],
        city_code: &[u8],
        action_code: u32,
    ) -> bool {
        if province_code.is_empty() || city_code.is_empty() {
            return false;
        }
        let Ok(actor_text) = core::str::from_utf8(actor_cid_number) else {
            return false;
        };
        let Some(actor_code) = primitives::cid::code::institution_code_from_cid_number(actor_text)
        else {
            return false;
        };
        let role_subject = entitybase::RoleSubject {
            cid_number: actor_cid_number.to_vec(),
            role_code: actor_role_code.to_vec(),
        };
        if !RuntimeInstitutionRoleAuthorization::is_authorized(
            who,
            &role_subject,
            &entitybase::BusinessActionId {
                module_tag: entitybase::business_action::MODULE_ADDRESS_REGISTRY.to_vec(),
                action_code,
            },
            RolePermissionOperation::Propose,
        ) {
            return false;
        }

        if actor_code == primitives::cid::code::FRG {
            if province_code.len() != 2 {
                return false;
            }
            let mut code = [0_u8; 2];
            code.copy_from_slice(province_code);
            return actor_role_code
                == primitives::governance_skeleton::province_commissioner_role_code(code)
                    .as_slice();
        }

        const CITY_REGISTRY_CODE: primitives::cid::code::InstitutionCode = *b"CREG";
        if actor_code != CITY_REGISTRY_CODE {
            return false;
        }
        let Ok((issuer_province_code, issuer_city_code)) =
            primitives::cid::number::cid_scope_codes(actor_cid_number)
        else {
            return false;
        };
        // CREG 管理员只能更新本市地址。镇以下地址名称与完整地址仍走本市注册局。
        issuer_province_code.as_ref() == province_code && issuer_city_code.as_ref() == city_code
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_authority(action_code: u32) -> Option<address::benchmarks::Authority<AccountId>> {
        if action_code > 4 {
            return None;
        }
        // 复用现有公开身份基准接口选择真实FRG省专员，不复制其岗位或账户常量。
        let (caller, cid, role, province, _) =
            <RuntimeCitizenIdentityAuthority as identity::CitizenIdentityAuthority<
                AccountId,
                identity::pallet::SignatureOf<Runtime>,
            >>::benchmark_authority()?;
        let mut area = None;
        primitives::cid::china::area::for_each_area(|item| {
            if let primitives::cid::china::area::AreaItem::Town(town) = item {
                if area.is_none() && town.province_code.as_bytes() == province.as_slice() {
                    area = Some((
                        town.city_code.as_bytes().to_vec(),
                        town.town_code.as_bytes().to_vec(),
                    ));
                }
            }
        });
        let (city_code, town_code) = area?;
        let actor = address::benchmarks::Authority {
            caller,
            actor_cid_number: cid.to_vec().try_into().ok()?,
            actor_role_code: role.to_vec().try_into().ok()?,
            province_code: province.to_vec(),
            city_code,
            town_code,
        };
        let authorized = || {
            if action_code == 0 {
                Self::can_update_catalog(
                    &actor.caller,
                    actor.actor_cid_number.as_slice(),
                    actor.actor_role_code.as_slice(),
                    action_code,
                )
            } else {
                // 删除用例先真实写入记录，因此同一主体必须具备写入及删除权限。
                (1..=4).all(|action| {
                    Self::can_update_address(
                        &actor.caller,
                        actor.actor_cid_number.as_slice(),
                        actor.actor_role_code.as_slice(),
                        &actor.province_code,
                        &actor.city_code,
                        action,
                    )
                })
            }
        };
        if !authorized() {
            // 只在基准externalities准备缺失状态，唯一写入实现仍归genesis所有。
            // 不修改创世实现、不直接写兄弟模块storage、不跳过正式岗位授权。
            ::genesis::institution::build::<Runtime>();
        }
        if !authorized() {
            return None;
        }
        Some(actor)
    }
}

impl address::Config for Runtime {
    type WeightInfo = address::weights::SubstrateWeight<Runtime>;
    type RuntimeEvent = RuntimeEvent;
    type AddressAuthority = RuntimeAddressAuthority;
    type MaxCodeLen = ConstU32<16>;
    type MaxVersionLen = ConstU32<32>;
    type MaxAddressNameCodeLen = ConstU32<3>;
    type MaxAddressLocalNoLen = ConstU32<4>;
    type MaxAddressNameLen = ConstU32<96>;
    type MaxAddressDetailLen = ConstU32<128>;
}

/// 公民身份四个签名域共用的账户验签实现。
///
/// benchmark 与生产 runtime 必须执行同一函数；feature 只能改变 benchmark 夹具，
/// 绝不能改变密码学验证结果。
fn verify_citizen_identity_account_signature(
    account_id: &AccountId,
    payload: &[u8],
    signature: &identity::pallet::SignatureOf<Runtime>,
    op_tag: u8,
) -> bool {
    let message = primitives::sign::signing_message(op_tag, payload);
    mldsa::verify_account::<Runtime>(account_id, &message, signature.as_slice())
}

/// FRAME benchmark 的临时 sr25519 签名夹具。
///
/// 使用 Polkadot SDK host crypto 把私钥只放在 benchmark externalities keystore；
/// runtime 代码和链上 Storage 均不保存 benchmark 私钥。
#[cfg(feature = "runtime-benchmarks")]
pub struct RuntimeCitizenIdentityBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl identity::BenchmarkHelper<AccountId, identity::pallet::SignatureOf<Runtime>>
    for RuntimeCitizenIdentityBenchmarkHelper
{
    fn signer() -> (sr25519::Public, AccountId) {
        let public = sp_io::crypto::sr25519_generate(0.into(), None);
        let account_id = MultiSigner::Sr25519(public).into_account();
        (public, account_id)
    }

    #[allow(clippy::expect_used)]
    fn sign(signer: &sr25519::Public, message: &[u8]) -> identity::pallet::SignatureOf<Runtime> {
        // BenchmarkHelper trait 不返回 Result；夹具缺失或越界属于基准环境配置错误，必须停止。
        let signature = sp_io::crypto::sr25519_sign(0.into(), signer, message)
            .expect("benchmark keystore must contain the generated sr25519 signer");
        signature
            .0
            .to_vec()
            .try_into()
            .expect("sr25519 signature must fit the runtime signature bound")
    }
}

// 机构自定义账户关闭由签名账户提交明确 CID 与岗位码；业务 pallet 通过统一岗位授权
// 查询校验人员的有效任职及岗位的具体业务权限。

/// 完整 CID 的顶层业务能力策略：固定机构、注册局与立法业务各走共享目录，
/// 其它普通机构仍只开放自身治理和账户生命周期；顶层允许不授予岗位权限。
pub struct RuntimeInstitutionCapabilityPolicy;

impl entitybase::InstitutionCapabilityPolicy for RuntimeInstitutionCapabilityPolicy {
    fn allows(
        cid_number: &[u8],
        business_action_id: &entitybase::BusinessActionId<Vec<u8>>,
        operation: entitybase::RolePermissionOperation,
    ) -> bool {
        let Ok(parts) = primitives::cid::number::parse_cid_number_parts_bytes(cid_number) else {
            return false;
        };
        let public_cid =
            publicentity::pallet::CidNumberOf::<Runtime>::try_from(cid_number.to_vec()).ok();
        let private_cid =
            privateentity::pallet::CidNumberOf::<Runtime>::try_from(cid_number.to_vec()).ok();
        let in_public = public_cid
            .as_ref()
            .is_some_and(publicentity::Institutions::<Runtime>::contains_key);
        let in_private = private_cid
            .as_ref()
            .is_some_and(privateentity::Institutions::<Runtime>::contains_key);
        let expected_module = match (in_public, in_private) {
            (true, false) => publicentity::MODULE_TAG,
            (false, true) => privateentity::MODULE_TAG,
            _ => return false,
        };
        if entitybase::fixed_institution_capability_allows(
            parts.institution,
            cid_number,
            business_action_id.module_tag.as_slice(),
            business_action_id.action_code,
            operation,
        ) {
            return true;
        }
        if in_public
            && entitybase::business_action::registry_institution_capability_allows(
                parts.institution,
                cid_number,
                business_action_id.module_tag.as_slice(),
                business_action_id.action_code,
                operation,
            )
        {
            return true;
        }
        if in_public
            && entitybase::business_action::legislation_institution_capability_allows(
                parts.institution,
                business_action_id.module_tag.as_slice(),
                business_action_id.action_code,
                operation,
            )
        {
            return true;
        }
        business_action_id.module_tag.as_slice() == expected_module
            && matches!(
                business_action_id.action_code,
                entitybase::business_action::ACTION_INSTITUTION_CLOSE
                    | entitybase::business_action::ACTION_INSTITUTION_GOVERNANCE
            )
    }
}

impl publicentity::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnAccountRegistered = crate::evm_accounts::EvmAccountMapping;
    type Currency = Balances;
    type InternalVoteEngine = InternalVote;
    type SiblingInstitutionQuery = PrivateEntity;
    type InstitutionCapabilityPolicy = RuntimeInstitutionCapabilityPolicy;
    type AccountValidator = RuntimeAccountValidator;
    type ReservedAccountChecker = RuntimeReservedAccountGuard;
    type ProtectedSourceChecker = RuntimeProtectedSourceChecker;
    type InstitutionAsset = RuntimeInstitutionAsset;
    type InstitutionQuery = RuntimeInstitutionQuery;
    type OnchainFeeCharger =
        onchain::OnchainExecutionFeeCharger<Runtime, Balances, OnchainExecutionFeeDistributor>;
    type RegistryAuthority = RuntimeRegistryAuthority;
    type MaxRoleEntries = MaxSnapshotVoters;
    type MaxCidNumberLength = ConstU32<{ primitives::core_const::CID_NUMBER_MAX_BYTES }>;
    type MaxAccountNameLength = ConstU32<128>;
    type MaxInstitutionAccounts = ConstU32<16>;
    type WeightInfo = publicentity::weights::SubstrateWeight<Runtime>;
}

impl privateentity::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnAccountRegistered = crate::evm_accounts::EvmAccountMapping;
    type Currency = Balances;
    type InternalVoteEngine = InternalVote;
    type SiblingInstitutionQuery = PublicEntity;
    type InstitutionCapabilityPolicy = RuntimeInstitutionCapabilityPolicy;
    type AccountValidator = RuntimeAccountValidator;
    type ReservedAccountChecker = RuntimeReservedAccountGuard;
    type ProtectedSourceChecker = RuntimeProtectedSourceChecker;
    type InstitutionAsset = RuntimeInstitutionAsset;
    type InstitutionQuery = RuntimeInstitutionQuery;
    type OnchainFeeCharger =
        onchain::OnchainExecutionFeeCharger<Runtime, Balances, OnchainExecutionFeeDistributor>;
    type RegistryAuthority = RuntimeRegistryAuthority;
    type ChainPhase = Genesis;
    type MaxRoleEntries = MaxSnapshotVoters;
    type MaxCidNumberLength = ConstU32<{ primitives::core_const::CID_NUMBER_MAX_BYTES }>;
    type MaxAccountNameLength = ConstU32<128>;
    type MaxInstitutionAccounts = ConstU32<16>;
    type WeightInfo = privateentity::weights::SubstrateWeight<Runtime>;
}

impl personal::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type InternalVoteEngine = InternalVote;
    type AccountValidator = RuntimeAccountValidator;
    type ReservedAccountChecker = RuntimeReservedAccountGuard;
    type ProtectedSourceChecker = RuntimeProtectedSourceChecker;
    type InstitutionAsset = RuntimeInstitutionAsset;
    type OnchainFeeCharger =
        onchain::OnchainExecutionFeeCharger<Runtime, Balances, OnchainExecutionFeeDistributor>;
    type MaxAccountNameLength = ConstU32<128>;
    type MaxPersonalAccountAdmins = MaxPersonalAccountAdmins;
    type MinCreateAmount = ConstU128<111>;
    type WeightInfo = crate::evm_accounts::EvmPersonalWeight;
}

pub struct RuntimeCitizenIdentityAuthority;

impl identity::CitizenIdentityAuthority<AccountId, identity::pallet::SignatureOf<Runtime>>
    for RuntimeCitizenIdentityAuthority
{
    fn can_manage_voting_identity(
        registrar: &AccountId,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        residence_province_code: &[u8],
        residence_city_code: &[u8],
        _level: identity::CitizenIdentityLevel,
        action_code: u32,
    ) -> bool {
        if residence_province_code.is_empty() || residence_city_code.is_empty() {
            return false;
        }
        let Ok(actor_text) = core::str::from_utf8(actor_cid_number) else {
            return false;
        };
        let Some(actor_code) = primitives::cid::code::institution_code_from_cid_number(actor_text)
        else {
            return false;
        };
        let role_subject = entitybase::RoleSubject {
            cid_number: actor_cid_number.to_vec(),
            role_code: actor_role_code.to_vec(),
        };
        let business_action_id = entitybase::BusinessActionId {
            module_tag: entitybase::business_action::MODULE_CITIZEN_IDENTITY.to_vec(),
            action_code,
        };
        if !RuntimeInstitutionRoleAuthorization::is_authorized(
            registrar,
            &role_subject,
            &business_action_id,
            RolePermissionOperation::Propose,
        ) {
            return false;
        }

        if actor_code == primitives::cid::code::FRG {
            let mut province = [0_u8; 2];
            if residence_province_code.len() != province.len() {
                return false;
            }
            province.copy_from_slice(residence_province_code);
            return actor_role_code
                == primitives::governance_skeleton::province_commissioner_role_code(province)
                    .as_slice();
        }

        const CITY_REGISTRY_CODE: primitives::cid::code::InstitutionCode = *b"CREG";
        if actor_code != CITY_REGISTRY_CODE {
            return false;
        }
        let Ok((registry_province_code, registry_city_code)) =
            primitives::cid::number::cid_scope_codes(actor_cid_number)
        else {
            return false;
        };
        // CREG 管理员只能管理本市公民身份；出生地不参与居住地注册权限。
        registry_province_code.as_ref() == residence_province_code
            && registry_city_code.as_ref() == residence_city_code
    }

    fn verify_citizen_signature(
        account_id: &AccountId,
        payload: &[u8],
        signature: &identity::pallet::SignatureOf<Runtime>,
    ) -> bool {
        verify_citizen_identity_account_signature(
            account_id,
            payload,
            signature,
            primitives::sign::OP_SIGN_CITIZEN_IDENTITY,
        )
    }

    fn verify_rebind_signature(
        account_id: &AccountId,
        payload: &[u8],
        signature: &identity::pallet::SignatureOf<Runtime>,
    ) -> bool {
        verify_citizen_identity_account_signature(
            account_id,
            payload,
            signature,
            primitives::sign::OP_SIGN_CID_REBIND,
        )
    }

    fn can_manage_anonymous_cid(
        registrar: &AccountId,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        action_code: u32,
    ) -> bool {
        let Ok(actor_text) = core::str::from_utf8(actor_cid_number) else {
            return false;
        };
        let Some(actor_code) = primitives::cid::code::institution_code_from_cid_number(actor_text)
        else {
            return false;
        };
        // 匿名 CID 为全国号、无省市归属:只要求发起主体是在册注册局(省级 FRG /
        // 市级 CREG)且持 identity 管理权,不做辖区匹配。
        const CITY_REGISTRY_CODE: primitives::cid::code::InstitutionCode = *b"CREG";
        if actor_code != primitives::cid::code::FRG && actor_code != CITY_REGISTRY_CODE {
            return false;
        }
        let role_subject = entitybase::RoleSubject {
            cid_number: actor_cid_number.to_vec(),
            role_code: actor_role_code.to_vec(),
        };
        let business_action_id = entitybase::BusinessActionId {
            module_tag: entitybase::business_action::MODULE_CITIZEN_IDENTITY.to_vec(),
            action_code,
        };
        RuntimeInstitutionRoleAuthorization::is_authorized(
            registrar,
            &role_subject,
            &business_action_id,
            RolePermissionOperation::Propose,
        )
    }

    fn verify_occupy_signature(
        account_id: &AccountId,
        payload: &[u8],
        signature: &identity::pallet::SignatureOf<Runtime>,
    ) -> bool {
        verify_citizen_identity_account_signature(
            account_id,
            payload,
            signature,
            primitives::sign::OP_SIGN_CID_OCCUPY,
        )
    }

    fn verify_registrar_rebind_signature(
        account_id: &AccountId,
        payload: &[u8],
        signature: &identity::pallet::SignatureOf<Runtime>,
    ) -> bool {
        verify_citizen_identity_account_signature(
            account_id,
            payload,
            signature,
            primitives::sign::OP_SIGN_CID_REGISTRAR_REBIND,
        )
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_authority() -> Option<(
        AccountId,
        identity::CidNumberBound,
        identity::RoleCodeBound,
        identity::AreaCodeBound,
        identity::AreaCodeBound,
    )> {
        // 基准 externalities 使用 fresh spec-genesis；这里选择 FRG 首个省专员岗位，
        // 不伪造“管理员即有权”的旁路。计时区间内仍由正式岗位权限目录完成授权。
        let province = primitives::cid::code::PROVINCE_CODE_INFOS
            .first()?
            .province_code;
        let institution = primitives::governance_skeleton::federal_registry_institution();
        let registrar = frame_benchmarking::account(institution.cid_number, 0, 0);
        Some((
            registrar,
            institution.cid_number.as_bytes().to_vec().try_into().ok()?,
            primitives::governance_skeleton::province_commissioner_role_code(province)
                .try_into()
                .ok()?,
            province.to_vec().try_into().ok()?,
            b"ZS01".to_vec().try_into().ok()?,
        ))
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_set_timestamp(timestamp_millis: u64) {
        pallet_timestamp::Pallet::<Runtime>::set_timestamp(timestamp_millis);
    }
}

impl identity::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnAccountRegistered = crate::evm_accounts::EvmAccountMapping;
    type MaxCitizenSignatureLength = ConstU32<{ mldsa::SIGNATURE_BYTES as u32 }>;
    type CitizenIdentityAuthority = RuntimeCitizenIdentityAuthority;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = RuntimeCitizenIdentityBenchmarkHelper;
    type OnVotingIdentityRegistered = CitizenIssue;
    type TimeProvider = crate::Timestamp;
    type MaxPopulationDaysPerBlock = ConstU32<366>;
    type MaxPopulationTransitionsPerBlock = ConstU32<2_048>;
    type MaxPopulationMaintenanceWeightPerBlock =
        identity::PopulationMaintenanceWeightFraction<Runtime, 8>;
    type WeightInfo = identity::weights::SubstrateWeight<Runtime>;
}

pub struct RuntimeSquarePostCitizenIdentity;

impl square::SquarePostCitizenIdentityProvider<AccountId> for RuntimeSquarePostCitizenIdentity {
    fn active_cid_number(account_id: &AccountId) -> Option<Vec<u8>> {
        let cid_number = identity::CidByAccountId::<Runtime>::get(account_id)?;
        if identity::AccountIdByCid::<Runtime>::get(&cid_number).as_ref() != Some(account_id) {
            return None;
        }
        let record = identity::CidRegistry::<Runtime>::get(&cid_number)?;
        (record.status == identity::CidRecordStatus::Active).then(|| cid_number.to_vec())
    }

    fn current_account_id(cid_number: &[u8]) -> Option<AccountId> {
        let cid_number = identity::CidNumberBound::try_from(cid_number.to_vec()).ok()?;
        let record = identity::CidRegistry::<Runtime>::get(&cid_number)?;
        if record.status != identity::CidRecordStatus::Active {
            return None;
        }
        let account_id = identity::AccountIdByCid::<Runtime>::get(&cid_number)?;
        (identity::CidByAccountId::<Runtime>::get(&account_id).as_ref() == Some(&cid_number))
            .then_some(account_id)
    }

    fn is_campaign_eligible(cid_number: &[u8], account_id: &AccountId) -> bool {
        <identity::Pallet<Runtime> as identity::CitizenIdentityProvider<
            AccountId,
        >>::candidate_subject(account_id, &identity::PopulationScope::Country)
        .is_some_and(|subject| {
            subject.cid_number.as_slice() == cid_number && subject.account_id == *account_id
        })
    }

    #[cfg(feature = "runtime-benchmarks")]
    #[allow(clippy::expect_used)]
    fn benchmark_seed_identity(account_id: &AccountId) -> Vec<u8> {
        // Benchmark 接口必须返回有效 CID；种子未形成双向绑定时立即停止，禁止伪造默认身份。
        <RuntimeCitizenIdentityReader as engingcore::CitizenIdentityReader<AccountId>>::benchmark_seed_identity(
            account_id,
            &identity::PopulationScope::Country,
        );
        Self::active_cid_number(account_id)
            .expect("square benchmark identity must be active and bidirectionally bound")
    }
}

impl square::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Identity = RuntimeSquarePostCitizenIdentity;
    type Currency = Balances;
    type TimeProvider = crate::Timestamp;
    type InstitutionAccountQuery = RuntimeInstitutionQuery;
    type InternalVoteEngine = InternalVote;
    type InstitutionRoleAuthorization = RuntimeInstitutionRoleAuthorization;
    type MaxSquarePostIdLen = ConstU32<64>;
    type MaxSquareCidNumberLen = ConstU32<32>;
    type MaxSquareStorageReceiptIdLen = ConstU32<96>;
    type MaxSubscriptionRenewalsPerBlock = ConstU32<50_000>;
    type WeightInfo = crate::evm_accounts::EvmSquareWeight;
}

impl citizenissue::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type WeightInfo = crate::evm_accounts::EvmCitizenWeight;
}

parameter_types! {
    /// 决议发行治理参数（统一来源于 primitives 常量）。
    pub const ResolutionIssuanceMaxReasonLen: u32 = primitives::count_const::RESOLUTION_ISSUANCE_MAX_REASON_LEN;
    pub const ResolutionIssuanceMaxAllocations: u32 = primitives::count_const::RESOLUTION_ISSUANCE_MAX_ALLOCATIONS;
    pub const ResolutionIssuanceMaxTotalIssuance: u128 = u128::MAX;
    pub const ResolutionIssuanceMaxSingleIssuance: u128 = 14_434_973_780_000;
    /// Runtime 升级治理提案备注最大长度。
    pub const RuntimeUpgradeMaxReasonLen: u32 = 1024;
    /// Runtime wasm 最大长度（字节）。
    pub const RuntimeUpgradeMaxCodeSize: u32 = 5 * 1024 * 1024;
    /// 管理员治理：单个注册机构账户管理员上限。
    ///
    /// 机构岗位快照覆盖1989人场景，不约束个人成员或个人冻结快照。
    pub const MaxSnapshotVoters: u32 = 1989;
    /// 管理员治理：单个个人账户管理员上限。
    pub const MaxPersonalAccountAdmins: u32 = primitives::multisig::MAX_PERSONAL_ADMINS;
    /// GRANDPA authority set 变更生效延迟（单位：区块）。
    /// 取非 0，给运维注入新 gran 私钥预留窗口，避免立即切换导致短时失票。
    pub const GrandpaAuthoritySetChangeDelay: u32 = 30;
}

impl destroy::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type InternalVoteEngine = InternalVote;
    type InstitutionRoleAuthorization = RuntimeInstitutionRoleAuthorization;
    type InstitutionQuery = RuntimeInstitutionQuery;
    type OnchainFeeCharger =
        onchain::OnchainExecutionFeeCharger<Runtime, Balances, OnchainExecutionFeeDistributor>;
    type WeightInfo = destroy::weights::SubstrateWeight<Runtime>;
}

impl grandpakey::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type GrandpaChangeDelay = GrandpaAuthoritySetChangeDelay;
    type InternalVoteEngine = InternalVote;
    type InstitutionRoleAuthorization = RuntimeInstitutionRoleAuthorization;
    type WeightInfo = grandpakey::weights::SubstrateWeight<Runtime>;
}

/// 执行期手续费分账适配器：把 `Currency` 产生的 `NegativeImbalance`
/// 转成统一分账器接收的 `Credit`（80% 全节点 / 10% 国家储委会 / 10% 安全基金）。
pub struct OnchainExecutionFeeDistributor;

impl frame_support::traits::OnUnbalanced<pallet_balances::NegativeImbalance<Runtime>>
    for OnchainExecutionFeeDistributor
{
    fn on_nonzero_unbalanced(amount: pallet_balances::NegativeImbalance<Runtime>) {
        use frame_support::traits::fungible::Balanced;
        // 将 NegativeImbalance 等额转换为统一分账器使用的 Credit。
        let value = frame_support::traits::Imbalance::peek(&amount);
        // 消费 NegativeImbalance，让付款账户的余额变化正式生效。
        drop(amount);
        // 用 Balanced trait 从“零”铸造等额 Credit 并交给统一分账器。
        // 注意：drop(NegativeImbalance) 已将资金从流通中移除，
        // issue() 再铸回等额 Credit 让 router 分配，总量不变。
        let credit = <Balances as Balanced<AccountId>>::issue(value);

        type DistributionRouter = onchain::OnchainFeeRouter<
            Runtime,
            Balances,
            PowDigestAuthor,
            RuntimeNrcAccountProvider,
            RuntimeSafetyFundAccountProvider,
        >;
        <DistributionRouter as frame_support::traits::tokens::imbalance::OnUnbalanced<_>>::on_unbalanced(credit);
    }
}

pub struct RuntimeMultisigWeights;

impl multisig::weights::WeightInfo for RuntimeMultisigWeights {
    fn propose_transfer() -> Weight {
        // 多签提案与执行都使用此权重上界，额外计入达标记录的一次读取。
        multisig::weights::SubstrateWeight::<Runtime>::propose_transfer()
            .saturating_add(Weight::from_parts(10_000_000, 3_000))
            .saturating_add(<Runtime as frame_system::Config>::DbWeight::get().reads(1))
            // Also bound background execution, outside the signed fee extension.
            .saturating_add(<crate::evm_accounts::EvmAccountMapping as frame_support::traits::Get<Weight>>::get().saturating_mul(4))
    }

    fn propose_safety_fund_transfer() -> Weight {
        multisig::weights::SubstrateWeight::<Runtime>::propose_safety_fund_transfer()
            // Also bound background execution, outside the signed fee extension.
            .saturating_add(<crate::evm_accounts::EvmAccountMapping as frame_support::traits::Get<Weight>>::get().saturating_mul(4))
    }

    fn propose_sweep_to_main() -> Weight {
        multisig::weights::SubstrateWeight::<Runtime>::propose_sweep_to_main()
            // Also bound background execution, outside the signed fee extension.
            .saturating_add(<crate::evm_accounts::EvmAccountMapping as frame_support::traits::Get<Weight>>::get().saturating_mul(4))
    }

    fn terminal_callback() -> Weight {
        multisig::weights::SubstrateWeight::<Runtime>::terminal_callback()
    }
}

impl multisig::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type InternalVoteEngine = InternalVote;
    type InstitutionRoleAuthorization = RuntimeInstitutionRoleAuthorization;
    type InstitutionAsset = RuntimeInstitutionAsset;
    type ProtectedSourceChecker = RuntimeProtectedSourceChecker;
    type MaxRemarkLen = ConstU32<256>;
    type OnchainFeeCharger =
        onchain::OnchainExecutionFeeCharger<Runtime, Balances, OnchainExecutionFeeDistributor>;
    // 多签配置查询使用个人完整模块与runtime机构聚合查询。
    // 转账治理时 multisig-transfer 通过 union 调用,先问个人侧、再问机构侧。
    type PersonalQuery = personal::Pallet<Runtime>;
    type InstitutionQuery = RuntimeInstitutionQuery;
    type WeightInfo = RuntimeMultisigWeights;
}

/// 机构生命周期聚合查询。
///
/// 下游交易模块只依赖本适配器；runtime 内部按公权、私权顺序查询两个生命周期 pallet。
pub struct RuntimeInstitutionQuery;

impl entitybase::InstitutionMultisigQuery<AccountId> for RuntimeInstitutionQuery {
    fn lookup_institution_account(cid_number: &[u8], account_name: &[u8]) -> Option<AccountId> {
        let public =
            publicentity::Pallet::<Runtime>::lookup_institution_account(cid_number, account_name);
        let private =
            privateentity::Pallet::<Runtime>::lookup_institution_account(cid_number, account_name);
        match (public, private) {
            (Some(account), None) | (None, Some(account)) => Some(account),
            _ => None,
        }
    }

    fn account_belongs_to(cid_number: &[u8], addr: &AccountId) -> bool {
        let public = publicentity::Pallet::<Runtime>::account_belongs_to(cid_number, addr);
        let private = privateentity::Pallet::<Runtime>::account_belongs_to(cid_number, addr);
        public ^ private
    }

    fn lookup_cid(addr: &AccountId) -> Option<Vec<u8>> {
        match (
            publicentity::Pallet::<Runtime>::lookup_cid(addr),
            privateentity::Pallet::<Runtime>::lookup_cid(addr),
        ) {
            (Some(cid), None) | (None, Some(cid)) => Some(cid),
            _ => None,
        }
    }

    fn lookup_org(addr: &AccountId) -> Option<engingcore::types::InstitutionCode> {
        match (
            publicentity::Pallet::<Runtime>::lookup_org(addr),
            privateentity::Pallet::<Runtime>::lookup_org(addr),
        ) {
            (Some(code), None) | (None, Some(code)) => Some(code),
            _ => None,
        }
    }

    fn account_exists(addr: &AccountId) -> bool {
        publicentity::Pallet::<Runtime>::account_exists(addr)
            ^ privateentity::Pallet::<Runtime>::account_exists(addr)
    }
}

/// 机构存在性统一按 CID 查询；公权、私权只是存储分区，不形成第二身份。
pub struct RuntimeInstitutionCidQuery;

impl entitybase::InstitutionCidQuery<engingcore::types::CidNumber> for RuntimeInstitutionCidQuery {
    fn cid_exists(cid_number: &engingcore::types::CidNumber) -> bool {
        publicentity::Institutions::<Runtime>::contains_key(cid_number)
            || privateentity::Institutions::<Runtime>::contains_key(cid_number)
    }
}

/// 通用机构治理结果路由适配器。
///
/// 已完成自身业务校验的任免/治理模块可用它按机构码选择 entity 模组；
/// `electionvote` 不使用本适配器，选举结果必须先回到创建提案的具体选举业务模块复核。
pub struct RuntimeInstitutionGovernanceResultHandler;

impl entitybase::InstitutionGovernanceResultHandler
    for RuntimeInstitutionGovernanceResultHandler
{
    fn apply_institution_governance_result(
        result: entitybase::InstitutionGovernanceResult,
    ) -> DispatchResult {
        match institution_role_directory(&result.cid_number) {
            Some(true) => publicentity::Pallet::<Runtime>::apply_institution_governance_result(result),
            Some(false) => privateentity::Pallet::<Runtime>::apply_institution_governance_result(result),
            None => Err(sp_runtime::DispatchError::Other("UnsupportedInstitutionGovernanceResultCode")),
        }
    }
}

/// 法定代表人及其当前钱包只从所属实体和 identity 查询。
pub struct RuntimeInstitutionLegalRepresentativeQuery;

impl entitybase::InstitutionLegalRepresentativeQuery<AccountId> for RuntimeInstitutionLegalRepresentativeQuery {
    fn legal_representative(cid: &[u8]) -> Option<AccountId> {
        let person_cid = Self::legal_representative_cid(cid)?.try_into().ok()?;
        identity::Pallet::<Runtime>::person_account(&person_cid)
    }

    fn legal_representative_cid(cid: &[u8]) -> Option<Vec<u8>> {
        match institution_role_directory(cid) {
            Some(true) => PublicEntity::legal_representative_cid(cid),
            Some(false) => PrivateEntity::legal_representative_cid(cid),
            None => None,
        }
    }
}

// 链下交易清算模块配置
/// CID 机构登记表查询实现。
///
/// 由 runtime 路由到所属公私实体，并以岗位业务权限校验签名。
pub struct MultisigCidAccountQuery;

impl offchain::bank_check::CidAccountQuery<AccountId> for MultisigCidAccountQuery {
    fn account_info(addr: &AccountId) -> Option<(Vec<u8>, Vec<u8>)> {
        publicentity::AccountRegisteredCid::<Runtime>::get(addr)
            .map(|info| (info.cid_number.to_vec(), info.account_name.to_vec()))
            .or_else(|| {
                privateentity::AccountRegisteredCid::<Runtime>::get(addr)
                    .map(|info| (info.cid_number.to_vec(), info.account_name.to_vec()))
            })
    }

    fn find_account(cid_number: &[u8], account_name: &[u8]) -> Option<AccountId> {
        let public_id: publicentity::CidNumberOf<Runtime> = cid_number.to_vec().try_into().ok()?;
        let public_name: publicentity::AccountNameOf<Runtime> =
            account_name.to_vec().try_into().ok()?;
        if let Some(info) =
            publicentity::InstitutionAccounts::<Runtime>::get(&public_id, &public_name)
        {
            return Some(info.account_id);
        }

        let private_id: privateentity::CidNumberOf<Runtime> =
            cid_number.to_vec().try_into().ok()?;
        let private_name: privateentity::AccountNameOf<Runtime> =
            account_name.to_vec().try_into().ok()?;
        privateentity::InstitutionAccounts::<Runtime>::get(&private_id, &private_name)
            .map(|info| info.account_id)
    }

    fn account_exists(addr: &AccountId) -> bool {
        RuntimeInstitutionQuery::account_exists(addr)
    }

    fn is_institution_role_authorized(
        cid_number: &[u8],
        role_code: &[u8],
        who: &AccountId,
        action_code: u32,
    ) -> bool {
        RuntimeInstitutionRoleAuthorization::is_authorized(
            who,
            &entitybase::RoleSubject {
                cid_number: cid_number.to_vec(),
                role_code: role_code.to_vec(),
            },
            &entitybase::BusinessActionId {
                module_tag: entitybase::business_action::MODULE_OFFCHAIN.to_vec(),
                action_code,
            },
            RolePermissionOperation::Propose,
        )
    }

    /// 清算行资格由身份注册局 eligible-search 负责筛选。
    /// 链上不保存 subject_property/sub_type/parent_cid_number，这里只确认该地址属于已登记的
    /// CID 机构账户,避免把 CID 内部机构类型字段重复落到链上。
    fn is_clearing_bank_eligible(addr: &AccountId) -> bool {
        RuntimeInstitutionQuery::account_exists(addr)
    }

    /// 判定 `bank` 主账户对应的机构是否
    /// 已声明为清算行节点(链上 `ClearingBankNodes` 存在该 cid_number 记录)。
    fn is_registered_clearing_node(bank: &AccountId) -> bool {
        let Some((cid_number, _account_name)) = Self::account_info(bank) else {
            return false;
        };
        let key: offchain::InstitutionCidNumber = match cid_number.try_into() {
            Ok(b) => b,
            Err(_) => return false,
        };
        offchain::pallet::ClearingBankNodes::<Runtime>::contains_key(&key)
    }
}

impl offchain::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type MaxBatchSize = ConstU32<100_000>;
    type MaxBatchSignatureLength = ConstU32<128>;
    type InstitutionAsset = RuntimeInstitutionAsset;
    type CidAccountQuery = MultisigCidAccountQuery;
    type OnchainFeeCharger =
        onchain::OnchainExecutionFeeCharger<Runtime, Balances, OnchainExecutionFeeDistributor>;
    type WeightInfo = offchain::weights::SubstrateWeight<Runtime>;
}

pub struct EnsureNrcMember;

#[cfg(feature = "runtime-benchmarks")]
fn seed_benchmark_identities(wallets: &[AccountId]) {
    for who in wallets {
        <RuntimeCitizenIdentityReader as engingcore::CitizenIdentityReader<AccountId>>::benchmark_seed_identity(who, &engingcore::PopulationScope::Country);
    }
}

#[cfg(feature = "runtime-benchmarks")]
fn seed_benchmark_public_seats(
    cid_number: &'static str,
    institution_code: primitives::cid::code::InstitutionCode,
) -> Result<AccountId, ()> {
    let mut offset = 0;
    for role in primitives::governance_skeleton::fixed_role_specs(institution_code) {
        if role.role_code == primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE {
            continue;
        }
        let end = offset + role.seats;
        let wallets: Vec<AccountId> = (offset..end)
            .map(|index| frame_benchmarking::account(cid_number, index, 0))
            .collect();
        seed_benchmark_identities(&wallets);
        PublicEntity::benchmark_seed_role_assignments(cid_number.as_bytes(), role.role_code, wallets)
            .map_err(|_| ())?;
        offset = end;
    }
    Ok(frame_benchmarking::account(cid_number, 0, 0))
}

#[cfg(feature = "runtime-benchmarks")]
fn seed_benchmark_joint_members_origin() -> Result<RuntimeOrigin, ()> {
    use primitives::cid::{china::{china_cb::CHINA_CB, china_ch::CHINA_CH}, code::{NRC, PRC, PRB}};

    let nrc = CHINA_CB.first().ok_or(())?;
    let cid = publicentity::CidNumberOf::<Runtime>::try_from(nrc.cid_number.as_bytes().to_vec())
        .map_err(|_| ())?;
    if !publicentity::Institutions::<Runtime>::contains_key(&cid) {
        ::genesis::institution::build::<Runtime>();
    }
    // 各机构的合成钱包进入真实的身份和任职校验路径。
    let proposer = seed_benchmark_public_seats(nrc.cid_number, NRC)?;
    for entry in CHINA_CB.iter().skip(1) {
        seed_benchmark_public_seats(entry.cid_number, PRC)?;
    }
    for entry in CHINA_CH {
        seed_benchmark_public_seats(entry.cid_number, PRB)?;
    }
    Ok(RuntimeOrigin::from(frame_system::RawOrigin::Signed(proposer)))
}

impl EnsureOrigin<RuntimeOrigin> for EnsureNrcMember {
    type Success = AccountId;

    fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
        let who = frame_system::EnsureSigned::<AccountId>::try_origin(o)?;
        if is_nrc_member(&who) {
            Ok(who)
        } else {
            Err(RuntimeOrigin::from(frame_system::RawOrigin::Signed(who)))
        }
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
        seed_benchmark_joint_members_origin()
    }
}

pub(crate) fn is_nrc_member(who: &AccountId) -> bool {
    // NRC 是固定创世机构，清单缺失属于不可恢复的 runtime 配置错误。
    let nrc_cid_number = primitives::cid::china::china_cb::CHINA_CB
        .first()
        .map(|n| n.cid_number.as_bytes())
        .unwrap_or_else(|| panic!("NRC CID must exist"));

    PublicEntity::is_active_assignment(nrc_cid_number, who, primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER)
}

/// 联合提案发起权限：国家储委会（CHINA_CB[0]）+ 43个省储委会（CHINA_CB[1..44]）。
pub struct EnsureJointProposer;

impl EnsureOrigin<RuntimeOrigin> for EnsureJointProposer {
    type Success = AccountId;

    fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
        frame_system::EnsureSigned::<AccountId>::try_origin(o)
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
        seed_benchmark_joint_members_origin()
    }
}

impl resolutionissue::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type ProposeOrigin = EnsureJointProposer;
    type WeightInfo = resolutionissue::weights::SubstrateWeight<Runtime>;
    type JointVoteEngine = JointVote;
    type InstitutionRoleAuthorization = publicentity::Pallet<Runtime>;
    type MaxReasonLen = ResolutionIssuanceMaxReasonLen;
    type MaxAllocations = ResolutionIssuanceMaxAllocations;
    type MaxTotalIssuance = ResolutionIssuanceMaxTotalIssuance;
    type MaxSingleIssuance = ResolutionIssuanceMaxSingleIssuance;
}

impl upgrade::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type ProposeOrigin = EnsureJointProposer;
    type DeveloperUpgradeOrigin = EnsureNrcMember;
    type JointVoteEngine = JointVote;
    type InstitutionRoleAuthorization = publicentity::Pallet<Runtime>;
    type RuntimeCodeExecutor = RuntimeSetCodeExecutor;
    type DeveloperUpgradeCheck = Genesis;
    type MaxReasonLen = RuntimeUpgradeMaxReasonLen;
    type MaxRuntimeCodeSize = RuntimeUpgradeMaxCodeSize;
    type WeightInfo = upgrade::weights::SubstrateWeight<Runtime>;
}

pub struct RuntimeSetCodeExecutor;

impl upgrade::RuntimeCodeExecutor for RuntimeSetCodeExecutor {
    fn execute_runtime_code(
        code: &[u8],
        pow_params: difficulty::PowDifficultyParams,
        activate_at: u32,
    ) -> DispatchResult {
        #[cfg(feature = "runtime-benchmarks")]
        {
            // benchmark 需要衡量治理编排本身的真实路径，
            // PoW暂存执行真实实现；系统set_code另按SystemWeightInfo计费，此处不重复写:code。
            if code.is_empty() || pow_params.validate().is_err() || activate_at == 0 {
                Err(sp_runtime::DispatchError::Other("empty runtime code"))
            } else {
                super::Difficulty::stage_params(pow_params, activate_at)
            }
        }

        #[cfg(not(feature = "runtime-benchmarks"))]
        {
            super::Difficulty::stage_params(pow_params, activate_at)?;
            let set_code_call = frame_system::Call::<Runtime>::set_code {
                code: code.to_vec(),
            };
            set_code_call
                .dispatch_bypass_filter(frame_system::RawOrigin::Root.into())
                .map(|_| ())
                .map_err(|e| e.error)
        }
    }
}

parameter_types! {
    // 立法院模块边界常量(ADR-027,第1步)
    pub const LegislationMaxTitleLen: u32 = 256;
    pub const LegislationMaxTextLen: u32 = 8192; // 条/款正文(宪法部分条较长)
    pub const LegislationMaxClausesPerArticle: u32 = 50;
    pub const LegislationMaxArticlesPerSection: u32 = 200;
    pub const LegislationMaxSectionsPerChapter: u32 = 50;
    pub const LegislationMaxChaptersPerLaw: u32 = 50;
    pub const LegislationMaxPendingActivations: u32 = 100;
}

impl election::Config for Runtime {}

impl legislation::Config for Runtime {
    type ActivationWeightBudget = engingcore::BlockWeightFraction<Runtime, 32>;
    type RuntimeEvent = RuntimeEvent;
    // 立法投票引擎接真实 legislationvote sub-pallet(ADR-027 第2步),投票端到端流程打通。
    type LegislationVoteEngine = LegislationVote;
    type InstitutionCidQuery = RuntimeInstitutionCidQuery;
    type InstitutionRoleAuthorization = RuntimeInstitutionRoleAuthorization;
    type MaxTitleLen = LegislationMaxTitleLen;
    type MaxTextLen = LegislationMaxTextLen;
    type MaxClausesPerArticle = LegislationMaxClausesPerArticle;
    type MaxArticlesPerSection = LegislationMaxArticlesPerSection;
    type MaxSectionsPerChapter = LegislationMaxSectionsPerChapter;
    type MaxChaptersPerLaw = LegislationMaxChaptersPerLaw;
    type MaxPendingActivations = LegislationMaxPendingActivations;
    type WeightInfo = legislation::weights::SubstrateWeight<Runtime>;
}

pub struct RuntimeJointVoteResultCallback;

impl engingcore::JointVoteResultCallback for RuntimeJointVoteResultCallback {
    fn execution_weight() -> Weight {
        use resolutionissue::weights::WeightInfo as _;
        use upgrade::weights::WeightInfo as _;
        let value = upgrade::weights::SubstrateWeight::<Runtime>::propose_runtime_upgrade()
            .max(resolutionissue::weights::SubstrateWeight::<Runtime>::propose_issuance());
        // Preserve the existing callback baseline; append the new mapping proof
        // budget after its proof-size reset so the extra cost is not discarded.
        Weight::from_parts(value.ref_time(), 0)
            .saturating_add(<crate::evm_accounts::EvmAccountMapping as frame_support::traits::Get<Weight>>::get()
                .saturating_mul(ResolutionIssuanceMaxAllocations::get() as u64),
        )
    }
    fn on_joint_vote_finalized(
        vote_proposal_id: u128,
        approved: bool,
    ) -> Result<engingcore::ProposalExecutionOutcome, sp_runtime::DispatchError> {
        #[cfg(feature = "runtime-benchmarks")]
        {
            let _ = (vote_proposal_id, approved);
            Ok(engingcore::ProposalExecutionOutcome::Ignored)
        }

        #[cfg(not(feature = "runtime-benchmarks"))]
        {
            if resolutionissue::Pallet::<Runtime>::owns_proposal(vote_proposal_id) {
                return <ResolutionIssue as engingcore::JointVoteResultCallback>::on_joint_vote_finalized(
                vote_proposal_id,
                approved,
            );
            }

            if upgrade::Pallet::<Runtime>::owns_proposal(vote_proposal_id) {
                return <Upgrade as engingcore::JointVoteResultCallback>::on_joint_vote_finalized(
                    vote_proposal_id,
                    approved,
                );
            }

            Err(sp_runtime::DispatchError::Other(
                "joint vote proposal not found in any module",
            ))
        }
    }
}

impl engingcore::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type MaxVoteNonceLength = ConstU32<64>;
    type MaxVoteSignatureLength = ConstU32<64>;
    type MaxAutoFinalizePerBlock = ConstU32<2_048>;
    type MaxAutoFinalizeWeightPerBlock = VotingMaintenanceBudget;
    type MaxExecutionWeightPerBlock = VotingMaintenanceBudget;
    type MaxCleanupWeightPerBlock = VotingMaintenanceBudget;
    type MaxProposalsPerExpiry = ConstU32<2_048>;
    type MaxInternalProposalMutexBindings = ConstU32<256>;
    type MaxActiveProposals = ConstU32<10>;
    type MaxProposalDataLen = ConstU32<{ 100 * 1024 }>;
    type MaxProposalObjectLen = ConstU32<{ 10 * 1024 * 1024 }>;
    type MaxModuleTagLen = ConstU32<32>;
    type MaxManualExecutionAttempts = ConstU32<3>;
    type ExecutionRetryGraceBlocks = VotingExecutionRetryGraceBlocks;
    type MaxExecutionRetryDeadlinesPerBlock = ConstU32<2_048>;
    type MaxPendingRetryExpirationsPerBlock = ConstU32<256>;
    type MaxCleanupStepsPerBlock = ConstU32<8>;
    type MaxCleanupActivationsPerBlock = ConstU32<64>;
    type CleanupKeysPerStep = ConstU32<256>;
    type CitizenIdentityReader = RuntimeCitizenIdentityReader;
    type JointVoteResultCallback = RuntimeJointVoteResultCallback;
    // 内部投票终态回调注册 6 个顶层槽位；公权/私权机构生命周期共用一个 tuple 槽位，
    // 个人多签生命周期与成员治理由同一个personal回调处理。
    // 顺序按调用频率降序:transfer / multisig manage 类业务最频繁,
    // grandpa key 替换最稀有放最后(tuple iterate 时命中越早越省 gas)。
    // 每个 Executor 通过 MODULE_TAG 前缀 + 独立存储键互斥认领本模块提案,
    // 非己方提案直接 Ok(()) skip,顺序不影响行为正确性。
    type InternalVoteResultCallback = (
        multisig::InternalVoteExecutor<Runtime>,
        (
            publicentity::InternalVoteExecutor<Runtime>,
            privateentity::InternalVoteExecutor<Runtime>,
        ),
        personal::InternalVoteExecutor<Runtime>,
        destroy::InternalVoteExecutor<Runtime>,
        grandpakey::InternalVoteExecutor<Runtime>,
        square::InternalVoteExecutor<Runtime>,
    );
    type InternalVoteProvider = RuntimeInternalVoteProvider;
    type MaxSnapshotVoters = MaxSnapshotVoters;
    type TimeProvider = pallet_timestamp::Pallet<Runtime>;
    type WeightInfo = engingcore::weights::SubstrateWeight<Runtime>;
    // 四类 timeout / cleanup / mode 终态副作用通过递归 Track tuple 派发。
    type TrackHandlers = (
        InternalVote,
        (JointVote, (LegislationVote, (ElectionVote, ()))),
    );
    // 立法投票(ADR-027):终态业务回调接 legislation，Track 接 legislationvote。
    // ProposalOwner 决定由法律、任免或预算业务认领；B1 先装配法律业务壳。
    type LegislationVoteResultCallback = (Legislation,);
    type ElectionVoteResultCallback = ElectionVote;
}

// Sub-pallet Config 注入。共用基础设施 engingcore::Config 已 impl 完;
// sub-pallet 各自 Config 需 RuntimeEvent + 自家 WeightInfo。
impl internalvote::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = RuntimeInstitutionRoleProvider;
    type WeightInfo = internalvote::weights::SubstrateWeight<Runtime>;
}

impl jointvote::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = RuntimeInstitutionRoleProvider;
    type WeightInfo = jointvote::weights::SubstrateWeight<Runtime>;
}

impl electionvote::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type MaxElectionCandidates = ConstU32<256>;
    // 互选选民来自 VotePlan 指定岗位的有效任职快照，不从 admins 推导。
    type InstitutionRoleProvider = RuntimeInstitutionRoleProvider;
    type WeightInfo = electionvote::weights::SubstrateWeight<Runtime>;
}

impl legislationvote::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = RuntimeInstitutionRoleProvider;
    type WeightInfo = legislationvote::weights::SubstrateWeight<Runtime>;
}

impl difficulty::Config for Runtime {
    type WeightInfo = difficulty::weights::SubstrateWeight<Runtime>;
}

frame_support::parameter_types! {
    pub const MaxDeclarationLen: u32 = 2048;
}

/// 创世机构 seeding 注入实现:runtime 侧调用 institution::build。
/// Runtime 装配公私实体，可直接复用创世规格播种器，
/// 因此治理耦合留在 runtime 层,不再作为 genesis pallet Config 的 supertrait。
pub struct RuntimeGenesisSeeder;
impl ::genesis::GenesisInstitutionSeeder for RuntimeGenesisSeeder {
    fn seed() {
        ::genesis::institution::build::<Runtime>();
    }
}

impl ::genesis::Config for Runtime {
    type WeightInfo = ::genesis::weights::SubstrateWeight<Runtime>;
    type MaxDeclarationLen = MaxDeclarationLen;
    type InstitutionSeeder = RuntimeGenesisSeeder;
}

pub struct RuntimeInternalVoteProvider;

impl engingcore::InternalVoteProvider<AccountId> for RuntimeInternalVoteProvider {
    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_seed_institution_voter(cid_number: &[u8], voter: &AccountId) {
        <RuntimeCitizenIdentityReader as engingcore::CitizenIdentityReader<AccountId>>::benchmark_seed_identity(voter, &engingcore::PopulationScope::Country);
        let cid = engingcore::CidNumber::try_from(cid_number.to_vec()).expect("基准机构 CID");
        let info = publicentity::Institutions::<Runtime>::get(&cid).expect("基准机构必须已登记");
        let spec = primitives::institution_constraints::member_composition_by_identity(info.institution_code, &cid);
        if let Some(spec) = spec {
            let mut wallets = (0..spec.max_members.saturating_sub(1))
                .map(|i| frame_benchmarking::account::<AccountId>("role-voter", i, 0)).collect::<Vec<_>>();
            wallets.push(voter.clone());
            seed_benchmark_identities(&wallets);
            PublicEntity::benchmark_seed_role(&cid, spec.role_code,
                entitybase::business_action::ACTION_ENACT_LAW, wallets).expect("基准必须配置合法岗位任职");
        } else {
            let role = primitives::governance_skeleton::fixed_role_specs(info.institution_code)
                .into_iter().find(|role| role.role_code != primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE)
                .expect("基准机构必须定义固定岗位");
            let mut wallets = (0..role.seats.saturating_sub(1))
                .map(|i| frame_benchmarking::account::<AccountId>("role-voter", i, 0)).collect::<Vec<_>>();
            wallets.push(voter.clone());
            seed_benchmark_identities(&wallets);
            PublicEntity::benchmark_seed_role_assignments(&cid, role.role_code, wallets).expect("基准必须配置合法岗位任职");
        }
    }

    fn institution_threshold(
        institution_code: engingcore::types::InstitutionCode,
        cid_number: &[u8],
    ) -> Option<u32> {
        let cid = engingcore::types::CidNumber::try_from(cid_number.to_vec()).ok()?;
        if cid_institution_code(&cid)? != institution_code {
            return None;
        }
        match institution_role_directory(&cid) {
            Some(true) => publicentity::InstitutionGovernanceThresholds::<Runtime>::get(cid),
            Some(false) => privateentity::InstitutionGovernanceThresholds::<Runtime>::get(cid),
            None => None,
        }
    }

    fn is_pending_personal_admin(personal_account_id: AccountId, who: &AccountId) -> bool {
        personal::Pallet::<Runtime>::is_pending_account_admin_for_snapshot(
            engingcore::types::PMUL,
            personal_account_id,
            who,
        )
    }

    fn get_pending_personal_admins(
        personal_account_id: AccountId,
    ) -> Option<alloc::vec::Vec<AccountId>> {
        personal::Pallet::<Runtime>::pending_account_admins_for_snapshot(
            engingcore::types::PMUL,
            personal_account_id,
        )
    }

    fn is_personal_admin(personal_account_id: AccountId, who: &AccountId) -> bool {
        personal::Pallet::<Runtime>::is_active_account_admin(
            engingcore::types::PMUL,
            personal_account_id,
            who,
        )
    }

    fn get_personal_admins(personal_account_id: AccountId) -> Option<Vec<AccountId>> {
        personal::Pallet::<Runtime>::active_account_admins(engingcore::types::PMUL, personal_account_id,
        )
    }

    fn legal_representative(cid_number: &[u8]) -> Option<AccountId> {
        <RuntimeInstitutionLegalRepresentativeQuery as entitybase::InstitutionLegalRepresentativeQuery<AccountId>>::legal_representative(
            cid_number,
        )
    }


}

/// 跨业务机构岗位授权路由；CID 必须只存在于公权或私权一个机构目录中。
pub struct RuntimeInstitutionRoleAuthorization;

impl InstitutionRoleAuthorizationQuery<AccountId> for RuntimeInstitutionRoleAuthorization {
    fn role_has_permission(
        role_subject: &entitybase::RoleSubject<Vec<u8>, Vec<u8>>,
        business_action_id: &entitybase::BusinessActionId<Vec<u8>>,
        operation: RolePermissionOperation,
    ) -> bool {
        match institution_role_directory(role_subject.cid_number.as_slice()) {
            Some(true) => <publicentity::Pallet<Runtime> as InstitutionRoleAuthorizationQuery<
                AccountId,
            >>::role_has_permission(
                role_subject, business_action_id, operation
            ),
            Some(false) => <privateentity::Pallet<Runtime> as InstitutionRoleAuthorizationQuery<
                AccountId,
            >>::role_has_permission(
                role_subject, business_action_id, operation
            ),
            None => false,
        }
    }

    fn is_authorized(
        who: &AccountId,
        role_subject: &entitybase::RoleSubject<Vec<u8>, Vec<u8>>,
        business_action_id: &entitybase::BusinessActionId<Vec<u8>>,
        operation: RolePermissionOperation,
    ) -> bool {
        match institution_role_directory(role_subject.cid_number.as_slice()) {
            Some(true) => <publicentity::Pallet<Runtime> as InstitutionRoleAuthorizationQuery<
                AccountId,
            >>::is_authorized(
                who, role_subject, business_action_id, operation
            ),
            Some(false) => <privateentity::Pallet<Runtime> as InstitutionRoleAuthorizationQuery<
                AccountId,
            >>::is_authorized(
                who, role_subject, business_action_id, operation
            ),
            None => false,
        }
    }

    fn role_subjects_with_permission(
        cid_number: &[u8],
        business_action_id: &entitybase::BusinessActionId<Vec<u8>>,
        operation: RolePermissionOperation,
    ) -> Vec<entitybase::RoleSubject<Vec<u8>, Vec<u8>>> {
        match institution_role_directory(cid_number) {
            Some(true) => <publicentity::Pallet<Runtime> as InstitutionRoleAuthorizationQuery<
                AccountId,
            >>::role_subjects_with_permission(
                cid_number, business_action_id, operation
            ),
            Some(false) => <privateentity::Pallet<Runtime> as InstitutionRoleAuthorizationQuery<
                AccountId,
            >>::role_subjects_with_permission(
                cid_number, business_action_id, operation
            ),
            None => Vec::new(),
        }
    }
}

/// 返回 `Some(true)` 表示公权、`Some(false)` 表示私权；不存在或双重登记均拒绝。
fn institution_role_directory(cid_number: &[u8]) -> Option<bool> {
    let public_cid =
        publicentity::pallet::CidNumberOf::<Runtime>::try_from(cid_number.to_vec()).ok();
    let private_cid =
        privateentity::pallet::CidNumberOf::<Runtime>::try_from(cid_number.to_vec()).ok();
    let in_public = public_cid
        .as_ref()
        .is_some_and(publicentity::Institutions::<Runtime>::contains_key);
    let in_private = private_cid
        .as_ref()
        .is_some_and(privateentity::Institutions::<Runtime>::contains_key);
    match (in_public, in_private) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

/// 内部投票与联合投票统一读取公权或私权机构的岗位任职快照。
pub struct RuntimeInstitutionRoleProvider;

impl engingcore::InstitutionRoleProvider<AccountId> for RuntimeInstitutionRoleProvider {
    fn is_active_assignment(cid_number: &[u8], who: &AccountId, role_code: &[u8]) -> bool {
        match institution_role_directory(cid_number) {
            Some(true) => <publicentity::Pallet<Runtime> as InstitutionRoleQuery<
                AccountId,
            >>::is_active_assignment(cid_number, who, role_code),
            Some(false) => <privateentity::Pallet<Runtime> as InstitutionRoleQuery<
                AccountId,
            >>::is_active_assignment(cid_number, who, role_code),
            None => false,
        }
    }

    fn active_persons_for_role(cid_number: &[u8], role_code: &[u8]) -> Vec<Vec<u8>> {
        match institution_role_directory(cid_number) {
            Some(true) => {
                publicentity::Pallet::<Runtime>::snapshot_role_persons(cid_number, role_code)
            }
            Some(false) => {
                privateentity::Pallet::<Runtime>::snapshot_role_persons(cid_number, role_code)
            }
            None => Vec::new(),
        }
    }
}

pub struct RuntimeCitizenIdentityReader;

impl engingcore::CitizenIdentityReader<AccountId> for RuntimeCitizenIdentityReader {
    fn person_cid(who: &AccountId) -> Option<engingcore::types::CidNumber> {
        identity::Pallet::<Runtime>::person_cid(who)
    }

    fn person_account(person_cid: &engingcore::types::CidNumber) -> Option<AccountId> {
        identity::Pallet::<Runtime>::person_account(person_cid)
    }

    fn citizen_subject(who: &AccountId) -> Option<identity::CitizenSubject<AccountId>> {
        <identity::Pallet<Runtime> as identity::CitizenIdentityProvider<AccountId>>::citizen_subject(
            who,
        )
    }

    fn voting_subject(
        who: &AccountId,
        scope: &identity::PopulationScope,
    ) -> Option<identity::CitizenSubject<AccountId>> {
        <identity::Pallet<Runtime> as identity::CitizenIdentityProvider<AccountId>>::voting_subject(
            who, scope,
        )
    }

    fn candidate_subject(
        who: &AccountId,
        scope: &identity::PopulationScope,
    ) -> Option<identity::CitizenSubject<AccountId>> {
        <identity::Pallet<Runtime> as identity::CitizenIdentityProvider<
            AccountId,
        >>::candidate_subject(who, scope)
    }

    fn population_data(scope: &identity::PopulationScope) -> Option<identity::PopulationData> {
        <identity::Pallet<Runtime> as identity::CitizenIdentityProvider<AccountId>>::population_data(
            scope,
        )
    }

    fn voting_subject_at(
        who: &AccountId,
        population_data: &identity::PopulationData,
    ) -> Option<identity::CitizenSubject<AccountId>> {
        <identity::Pallet<Runtime> as identity::CitizenIdentityProvider<
            AccountId,
        >>::voting_subject_at(who, population_data)
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_seed_identity(who: &AccountId, scope: &identity::PopulationScope) {
        use identity::{
            AccountIdByCid, CandidateIdentity, CandidateIdentityByCid, CidByAccountId, CidRecord,
            CidRecordStatus, CitizenStatus, CountryVotingCount, NextEligibilityRevision,
            PopulationReadyDate, VotingEligibilityVersion, VotingEligibilityVersionCount,
            VotingEligibilityVersions, VotingIdentity, VotingIdentityByCid,
        };

        // identity 按 timestamp 校验护照窗口；benchmark externalities 的
        // 创世时间为 0，先推进到稳定的 2027 年时间点。
        pallet_timestamp::Pallet::<Runtime>::set_timestamp(1_800_000_000_000);
        PopulationReadyDate::<Runtime>::put(identity::Pallet::<Runtime>::current_date_int());
        let now = frame_system::Pallet::<Runtime>::block_number();
        // 每个 benchmark 账户必须得到不同的公民 CID；固定哈希长度不会超过协议边界。
        let cid_number: identity::CidNumberBound = CidByAccountId::<Runtime>::get(who)
            .unwrap_or_else(|| {
                let public_key = alloc::format!("{:?}", who);
                let province = primitives::cid::code::PROVINCE_CODE_INFOS[0];
                primitives::cid::generator::generate_cid_number(
                    primitives::cid::generator::GenerateCidNumberInput {
                        public_key: &public_key,
                        p1: "1",
                        province_code: core::str::from_utf8(&province.province_code).unwrap(),
                        province_name: province.province_name,
                        city_code: primitives::cid::generator::RESERVED_PROVINCE_CITY_CODE,
                        city_name: "匿名",
                        year: 2026,
                        institution: "CTZN",
                    },
                )
                .expect("基准工厂必须使用所属CID生成协议")
                .into_bytes()
                .try_into()
                .expect("公民CID必须满足链上长度")
            });
        let identity = VotingIdentity {
            passport_valid_from: 19700101,
            passport_valid_until: 29991231,
            citizen_status: CitizenStatus::Normal,
            residence_province_code: Default::default(),
            residence_city_code: Default::default(),
            residence_town_code: Default::default(),
            updated_at: now,
        };
        let revision = NextEligibilityRevision::<Runtime>::get().saturating_add(1);
        let version_index = VotingEligibilityVersionCount::<Runtime>::get(&cid_number);
        if version_index > 0 {
            VotingEligibilityVersions::<Runtime>::mutate(
                &cid_number,
                version_index.saturating_sub(1),
                |version| {
                    if let Some(version) = version {
                        version.valid_until_revision = Some(revision);
                    }
                },
            );
        }
        VotingEligibilityVersions::<Runtime>::insert(
            &cid_number,
            version_index,
            VotingEligibilityVersion {
                identity: identity.clone(),
                valid_from_revision: revision,
                valid_until_revision: None,
            },
        );
        VotingEligibilityVersionCount::<Runtime>::insert(
            &cid_number,
            version_index.saturating_add(1),
        );
        NextEligibilityRevision::<Runtime>::put(revision);
        VotingIdentityByCid::<Runtime>::insert(&cid_number, identity);
        AccountIdByCid::<Runtime>::insert(&cid_number, who);
        CidByAccountId::<Runtime>::insert(who, &cid_number);
        identity::CidRegistry::<Runtime>::insert(
            &cid_number,
            CidRecord {
                registrar_cid_number: b"benchmark-registrar"
                    .to_vec()
                    .try_into()
                    .unwrap_or_else(|error| panic!("bounded registrar CID: {error:?}")),
                commitment: [0u8; 32],
                residence_province_code: Default::default(),
                residence_city_code: Default::default(),
                status: CidRecordStatus::Active,
                registered_at: now,
                revoked_at: None,
            },
        );
        CandidateIdentityByCid::<Runtime>::insert(
            &cid_number,
            CandidateIdentity {
                birth_province_code: Default::default(),
                birth_city_code: Default::default(),
                birth_town_code: Default::default(),
                family_name: b"benchmark"
                    .to_vec()
                    .try_into()
                    .unwrap_or_else(|error| panic!("bounded family name: {error:?}")),
                given_name: b"citizen"
                    .to_vec()
                    .try_into()
                    .unwrap_or_else(|error| panic!("bounded given name: {error:?}")),
                citizen_sex: identity::CitizenSex::Male,
                birth_date: 20000101,
                updated_at: now,
            },
        );
        match scope {
            identity::PopulationScope::Country => CountryVotingCount::<Runtime>::put(1),
            identity::PopulationScope::Province(province) => {
                identity::ProvinceVotingCount::<Runtime>::insert(province, 1)
            }
            identity::PopulationScope::City(province, city) => {
                identity::CityVotingCount::<Runtime>::insert((province, city), 1)
            }
            identity::PopulationScope::Town(province, city, town) => {
                identity::TownVotingCount::<Runtime>::insert((province, city, town), 1)
            }
        }
    }
}
// pallet_assets 内核接入(ADR-011 第八节)+ OnchainIssue 外壳配置
//
// pallet_assets 是用户代币的内核 storage / 资产记账实现,
// **所有原生 extrinsic 在 RuntimeCallFilter 中 reject**。
// 业务调用必须经由 OnchainIssue::propose_* → InternalVote/JointVote callback →
// onchainissue 内部以 Root 调用 pallet_assets 的内核 API。
//
// pallet_assets 的 deposit 系列常量统一为 0，仅保留底层资产记账能力。
// 当前 OnchainIssue 对外调用仍由 RuntimeCallFilter 拒绝；后续业务实装时，
// 费用类型必须进入统一 FeeRoute，实际链上执行费必须复用统一执行收费接口，
// 不得恢复专用创建费、押金收费或其它旁路。

parameter_types! {
    /// 资产 metadata 字符串字段长度上限(name / symbol / description),
    /// 与 onchainissue::Config::MaxAssetNameLen 等参数对齐。
    pub const AssetsStringLimit: u32 = 64;
    /// 单批 destroy 时一次清理的账户/审批上限。
    pub const AssetsRemoveItemsLimit: u32 = 1000;
    /// pallet_assets 自身 deposit 系列常量；均设为 0，不承担业务收费职责。
    pub const AssetsDepositZero: Balance = 0;
}

impl pallet_assets::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Balance = Balance;
    type RemoveItemsLimit = AssetsRemoveItemsLimit;
    type AssetId = u32;
    type AssetIdParameter = codec::Compact<u32>;
    type Currency = Balances;
    /// 外部 extrinsic 全部被 RuntimeCallFilter reject,这里 origin 设啥不影响实际入口。
    /// CreateOrigin 接 EnsureSigned 仅为满足 trait Success=AccountId 约束;
    /// ForceOrigin 接 EnsureRoot(Success=())。OnchainIssue 内部经 fungibles trait
    /// (Create / Mutate)直接调内核 API,不走 extrinsic origin 路径。
    type CreateOrigin =
        frame_support::traits::AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountId>>;
    type ForceOrigin = frame_system::EnsureRoot<AccountId>;
    type AssetDeposit = AssetsDepositZero;
    type AssetAccountDeposit = AssetsDepositZero;
    type MetadataDepositBase = AssetsDepositZero;
    type MetadataDepositPerByte = AssetsDepositZero;
    type ApprovalDeposit = AssetsDepositZero;
    type StringLimit = AssetsStringLimit;
    type Freezer = ();
    type Holder = ();
    type Extra = ();
    type CallbackHandle = ();
    type ReserveData = ();
    type WeightInfo = pallet_assets::weights::SubstrateWeight<Runtime>;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = ();
}

parameter_types! {
    pub const OnchainAssetMaxNameLen: u32 = 64;
    pub const OnchainAssetMaxSymbolLen: u32 = 16;
    pub const OnchainAssetMaxDescriptionLen: u32 = 256;
    pub const OnchainAssetMaxBlacklistWordLen: u32 = 32;
    pub const OnchainAssetMaxBlacklistEntries: u32 = 256;
    pub const OnchainAssetReasonHashLen: u32 = 32;
    pub const OnchainAssetMaxScheduledPerBlock: u32 = 64;
}

impl onchainissue::pallet::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    /// pallet_assets 内核类型绑定。onchainissue 通过该类型调内核 create / mint_into 等内部 API,
    /// 不走原生 extrinsic(已被 RuntimeCallFilter 拦截)。
    type Assets = Assets;
    type InstitutionQuery = RuntimeInstitutionQuery;
    type InstitutionRoleAuthorization = RuntimeInstitutionRoleAuthorization;
    type MaxAssetNameLen = OnchainAssetMaxNameLen;
    type MaxAssetSymbolLen = OnchainAssetMaxSymbolLen;
    type MaxAssetDescriptionLen = OnchainAssetMaxDescriptionLen;
    type MaxBlacklistWordLen = OnchainAssetMaxBlacklistWordLen;
    type MaxBlacklistEntries = OnchainAssetMaxBlacklistEntries;
    type ReasonHashLen = OnchainAssetReasonHashLen;
    type MaxScheduledPerBlock = OnchainAssetMaxScheduledPerBlock;
    type WeightInfo = onchainissue::weights::ZeroWeight;
}

parameter_types! {
    /// 零存储押金，不增设任何收费或退款规则。
    pub const ReviveCodeHashLockupDepositPercent: Perbill = Perbill::zero();
    /// 资源上限固定取正常单笔预算的一半，与手续费无关。
    pub const ReviveMaxEthExtrinsicWeight: sp_runtime::FixedU128 =
        sp_runtime::FixedU128::from_rational(1, 2);
}

/// 只读报价与付款预检复用当前交易支付配置；实际扣款由同一收费器执行一次。
pub struct RuntimeNativeFee;

fn native_fee_error(
    error: sp_runtime::transaction_validity::TransactionValidityError,
) -> sp_runtime::transaction_validity::InvalidTransaction {
    match error {
        sp_runtime::transaction_validity::TransactionValidityError::Invalid(reason) => reason,
        _ => sp_runtime::transaction_validity::InvalidTransaction::Payment,
    }
}

impl pallet_revive::evm::fees::NativeFee<Runtime> for RuntimeNativeFee {
    type Charger = <Runtime as pallet_transaction_payment::Config>::OnChargeTransaction;

    fn quote(
        who: &AccountId,
        call: &RuntimeCall,
    ) -> Result<Balance, sp_runtime::transaction_validity::InvalidTransaction> {
        use frame_support::dispatch::GetDispatchInfo;
        onchain::charge_details::<Runtime, Balances, RuntimeFeeRouter>(
            who,
            call,
            &call.get_dispatch_info(),
            0,
        )
        .map(|details| details.map_or(0, |(_, fee)| fee))
        .map_err(native_fee_error)
    }

    fn validate(
        who: &AccountId,
        call: &RuntimeCall,
    ) -> Result<(), sp_runtime::transaction_validity::InvalidTransaction> {
        use frame_support::dispatch::GetDispatchInfo;
        use pallet_transaction_payment::OnChargeTransaction;
        <Self::Charger as OnChargeTransaction<Runtime>>::can_withdraw_fee(
            who,
            call,
            &call.get_dispatch_info(),
            0,
            0,
        )
        .map_err(native_fee_error)
    }
}

/// Ethereum 使用同一个整分余额账本；gas 仅约束执行资源，费用仍完全归原有制度。
impl pallet_revive::Config for Runtime {
    type Time = crate::Timestamp;
    type Balance = Balance;
    type Currency = Balances;
    type OnBurn = RuntimeDustHandler;
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeHoldReason = RuntimeHoldReason;
    type WeightInfo = pallet_revive::weights::SubstrateWeight<Runtime>;
    type Precompiles = ();
    type FindAuthor = PowDigestAuthor;
    type DepositPerByte = ConstU128<0>;
    type DepositPerItem = ConstU128<0>;
    type DepositPerChildTrieItem = ConstU128<0>;
    type CodeHashLockupDepositPercent = ReviveCodeHashLockupDepositPercent;
    type AddressMapper = pallet_revive::AccountId32Mapper<Runtime>;
    type AllowEVMBytecode = ConstBool<true>;
    type UploadOrigin = frame_system::EnsureSigned<AccountId>;
    type InstantiateOrigin = frame_system::EnsureSigned<AccountId>;
    type RuntimeMemory = ConstU32<{ 128 * 1024 * 1024 }>;
    type PVFMemory = ConstU32<{ 512 * 1024 * 1024 }>;
    type ChainId = ConstU64<{ primitives::core_const::ETHEREUM_CHAIN_ID }>;
    type NativeToEthRatio = ConstU64<{ primitives::core_const::NATIVE_TO_ETH_RATIO }>;
    type StrictNativeBalance = ConstBool<true>;
    type FeeInfo = pallet_revive::evm::fees::NativeInfo<
        crate::TransactionAddress,
        crate::Signature,
        crate::EthExtraImpl,
        RuntimeNativeFee,
    >;
    type Deposit = ();
    type MaxEthExtrinsicWeight = ReviveMaxEthExtrinsicWeight;
    type DebugEnabled = ConstBool<false>;
    type AutoMap = ConstBool<true>;
    type GasScale = ConstU32<10>;
}

#[cfg(feature = "runtime-benchmarks")]
impl publicentity::benchmarks::BenchmarkSetup for Runtime {
    fn registry() -> publicentity::benchmarks::Registry<AccountId> {
        let a=<RuntimeAddressAuthority as address::AddressUpdateAuthority<AccountId>>::benchmark_authority(1).expect("机构基准必须取得真实注册局权限");
        publicentity::benchmarks::Registry {
            caller: a.caller,
            cid: a.actor_cid_number.to_vec(),
            role: a.actor_role_code.to_vec(),
            province: a.province_code,
            city: a.city_code,
        }
    }
}

#[cfg(feature = "runtime-benchmarks")]
impl privateentity::benchmarks::BenchmarkSetup for Runtime {
    fn registry() -> privateentity::benchmarks::Registry<AccountId> {
        let a=<RuntimeAddressAuthority as address::AddressUpdateAuthority<AccountId>>::benchmark_authority(1).expect("机构基准必须取得真实注册局权限");
        privateentity::benchmarks::Registry {
            caller: a.caller,
            cid: a.actor_cid_number.to_vec(),
            role: a.actor_role_code.to_vec(),
            province: a.province_code,
            city: a.city_code,
        }
    }
}

#[cfg(feature = "runtime-benchmarks")]
impl legislation::benchmarks::BenchmarkSetup for Runtime {
    fn authority(action: u32) -> legislation::benchmarks::Authority<AccountId> {
        use entitybase::InstitutionRoleAuthorizationQuery;
        System::set_block_number(1);
        super::Timestamp::set_timestamp(1800000000000);
        let find = |code| {
            primitives::cid::china::china_lf::CHINA_LF
                .iter()
                .find(|entry| {
                    primitives::cid::code::institution_code_from_cid_number(entry.cid_number)
                        == Some(code)
                })
                .expect("真实立法机构必须存在")
        };
        let house = find(*b"NRP\0");
        let senate = find(*b"NSN\0");
        let legislature = find(*b"NLG\0");
        let house_cid: publicentity::CidNumberOf<Runtime> =
            house.cid_number.as_bytes().to_vec().try_into().unwrap();
        if !publicentity::Institutions::<Runtime>::contains_key(&house_cid) {
            ::genesis::institution::build::<Runtime>();
        }
        let mut proposer = None;
        for (index, entry) in [house, senate].into_iter().enumerate() {
            let cid: publicentity::CidNumberOf<Runtime> =
                entry.cid_number.as_bytes().to_vec().try_into().unwrap();
            let role = primitives::institution_constraints::member_composition_by_identity(
                primitives::cid::code::institution_code_from_cid_number(entry.cid_number).unwrap(),
                cid.as_slice(),
            )
            .expect("立法基准机构必须有成员岗位规格")
            .role_code
            .to_vec();
            let code =
                primitives::cid::code::institution_code_from_cid_number(entry.cid_number).unwrap();
            let seats = primitives::institution_constraints::member_composition_by_identity(
                code,
                cid.as_slice(),
            )
            .map(|spec| spec.max_members)
            .or_else(|| {
                primitives::governance_skeleton::fixed_role_seats_by_identity(
                    code,
                    cid.as_slice(),
                    role.as_slice(),
                )
            })
            .unwrap_or(MaxSnapshotVoters::get());
            let accounts = (0..seats).map(|i| frame_benchmarking::account::<AccountId>("law-member", i, index as u32)).collect::<Vec<_>>();
            seed_benchmark_identities(&accounts);
            PublicEntity::benchmark_seed_role(
                cid.as_slice(),
                role.as_slice(),
                action,
                accounts.clone(),
            )
            .unwrap();
            if index == 0 {
                proposer = Some((accounts[0].clone(), role.to_vec()));
            }
        }
        let (caller, role) = proposer.unwrap();
        assert!(RuntimeInstitutionRoleAuthorization::is_authorized(
            &caller,
            &entitybase::RoleSubject {
                cid_number: house_cid.to_vec(),
                role_code: role.clone()
            },
            &entitybase::BusinessActionId {
                module_tag: legislation::MODULE_TAG.to_vec(),
                action_code: action
            },
            entitybase::RolePermissionOperation::Propose
        ));
        <RuntimeCitizenIdentityReader as engingcore::CitizenIdentityReader<AccountId>>::benchmark_seed_identity(&caller,&engingcore::PopulationScope::Country);
        legislation::benchmarks::Authority {
            caller,
            actor: house_cid.to_vec(),
            role,
            houses: sp_runtime::sp_std::vec![
                house.cid_number.as_bytes().to_vec(),
                senate.cid_number.as_bytes().to_vec(),
            ],
            executive: primitives::cid::china::china_zf::CHINA_ZF[0]
                .cid_number
                .as_bytes()
                .to_vec(),
            legislature: legislature.cid_number.as_bytes().to_vec(),
        }
    }
}

#[cfg(feature = "runtime-benchmarks")]
impl engingcore::benchmarks::BenchmarkSetup for Runtime {
    fn recovery(cancel: bool) -> (u128, AccountId) {
        use codec::Decode;
        use frame_support::traits::Currency;
        use frame_system::RawOrigin;
        System::set_block_number(1);
        super::Timestamp::set_timestamp(1800000000000);
        let entry = &primitives::cid::china::china_cb::CHINA_CB[0];
        let proposer: AccountId = frame_benchmarking::account(entry.cid_number, 0, 0);
        let cid: engingcore::CidNumber = entry.cid_number.as_bytes().to_vec().try_into().unwrap();
        let role: engingcore::RoleCode = primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
            .to_vec()
            .try_into()
            .unwrap();
        let id = engingcore::Pallet::<Runtime>::next_proposal_id();
        if cancel {
                use ed25519_dalek::{Signer, SigningKey};
            let pair = SigningKey::from_bytes(&[117u8; 32]); // 公开确定性基准输入，不用于任何真实节点。
            let new_key = pair.verifying_key().to_bytes();
            let old_key = SigningKey::from_bytes(&[116; 32]).verifying_key().to_bytes();
            if let Some(old) = grandpakey::CurrentGrandpaKeys::<Runtime>::get(&cid) {
                grandpakey::GrandpaKeyOwnerByKey::<Runtime>::remove(old);
            }
            grandpakey::CurrentGrandpaKeys::<Runtime>::insert(&cid, old_key);
            grandpakey::GrandpaKeyOwnerByKey::<Runtime>::insert(old_key, &cid);
            pallet_grandpa::Authorities::<Runtime>::put(pallet_grandpa::BoundedAuthorityList::<
                <Runtime as pallet_grandpa::Config>::MaxAuthorities,
            >::force_from(alloc::vec![(sp_core::ed25519::Public::from_raw(old_key).into(), 1)], None));
            let expires = 100000u32;
            let payload = grandpakey::GrandpaKeyProofPayload {
                genesis_hash: System::block_hash(0),
                actor_cid_number: cid.clone(),
                actor_role_code: role.clone(),
                initiator_account_id: proposer.clone(),
                old_public_key: old_key,
                new_public_key: new_key,
                current_set_id: pallet_grandpa::Pallet::<Runtime>::current_set_id(),
                proof_nonce: 0,
                proof_expires_at: expires,
                change_kind: grandpakey::GrandpaKeyChangeKind::EmergencyRecovery,
            };
            let signature = pair
                .sign(&grandpakey::proof_signing_digest(&payload))
                .to_bytes();
            grandpakey::Pallet::<Runtime>::propose_emergency_grandpa_key_recovery(
                RawOrigin::Signed(proposer.clone()).into(),
                cid.clone(),
                role,
                new_key,
                0,
                expires,
                signature,
            )
            .unwrap();
            // 发起人退出后提案已不可执行；另一名原快照委员执行合法取消。
            let caller = frame_benchmarking::account(entry.cid_number, 1, 0);
            let accounts = (0..primitives::count_const::NRC_COMMITTEE_SEATS).map(|index| {
                let label = if index == 0 { "replacement" } else { entry.cid_number };
                frame_benchmarking::account(label, index, 0)
            }).collect::<Vec<_>>();
            seed_benchmark_identities(&accounts);
            PublicEntity::benchmark_seed_role_assignments(&cid, primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER, accounts).unwrap();
            (id, caller)
        } else {
            let fund =
                AccountId::decode(&mut &primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT[..])
                    .unwrap();
            let fee = AccountId::decode(&mut &entry.fee_account[..]).unwrap();
            let beneficiary =
                frame_benchmarking::account::<AccountId>("recovery-beneficiary", 0, 0);
            drop(Balances::make_free_balance_be(&fund, 1000000000));
            drop(Balances::make_free_balance_be(&fee, 1000000000));
            multisig::Pallet::<Runtime>::propose_safety_fund_transfer(
                RawOrigin::Signed(proposer.clone()).into(),
                cid,
                role,
                fund,
                beneficiary,
                111,
                sp_runtime::sp_std::vec![b'r';99].try_into().unwrap(),
            )
            .unwrap();
            (id, proposer)
        }
    }
}

impl mldsa::Config for Runtime {
    type ChainPhase = mldsa::GenesisChainPhase<Runtime>;
    type WeightInfo = mldsa::weights::SubstrateWeight<Runtime>;
}

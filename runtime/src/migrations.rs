//! 原链机构存储迁移。旧编码仅在本模块解码，正常交易只消费新布局。

use alloc::{collections::BTreeMap, vec::Vec};
use codec::{Decode, DecodeAll, Encode};
use frame_support::migrations::MultiStepMigrator;
use frame_support::{
    storage::{unhashed, with_transaction, TransactionOutcome},
    traits::{ConstU32, OnRuntimeUpgrade, PalletInfoAccess, StorageVersion},
    weights::{constants::RocksDbWeight, Weight},
    BoundedVec,
};
use primitives::{
    account_derive::{RESERVED_NAME_FEE, RESERVED_NAME_MAIN},
    cid::{code::InstitutionCode, number::parse_cid_number_parts_bytes},
    governance_skeleton::fixed_role_seats_by_identity,
    institution_constraints::{
        member_composition_by_identity, ROLE_CODE_LEGAL_REPRESENTATIVE,
        ROLE_NAME_LEGAL_REPRESENTATIVE,
    },
};
use sp_runtime::DispatchError;

use crate::{AccountId, BlockNumber, PrivateEntity, PublicEntity, Runtime};

pub(crate) const PROGRESS: &[u8] = b":citizenchain:entity-migration:progress:v1";
pub(crate) const RESULT: &[u8] = b":citizenchain:entity-migration:result:v1";
const VERSION: u16 = 1;
const OLD_CAPACITY: u32 = 1989;
const BATCH_LIMIT: u32 = 128;
const ACTIVE_CHECK_LIMIT: u32 = 512;
const SEAT_BATCH: usize = 24;
const MAX_LEGACY_ASSIGNMENT_BYTES: usize = 600_000;
const MAX_ROW_BYTES: usize = 700_000;
const INVALID: &str = "旧机构状态不符合迁移合同";

type Cid = publicentity::CidNumberOf<Runtime>;
type Name = publicentity::AccountNameOf<Runtime>;
type RoleCode = publicentity::RoleCodeOf;
type SourceRef = publicentity::institution::role::AssignmentSourceRefOf;
type Role = publicentity::InstitutionRoleOf<Runtime>;
type Assignment = publicentity::InstitutionAssignmentOf<Runtime>;
type Assignments = BoundedVec<Assignment, ConstU32<OLD_CAPACITY>>;
type Prefix = [u8; 32];

#[derive(Encode, Decode)]
pub(crate) struct OldRole {
    pub(crate) cid_number: Cid,
    pub(crate) role_code: RoleCode,
    pub(crate) role_name: Name,
    pub(crate) term_required: bool,
    pub(crate) role_status: entitybase::InstitutionRoleStatus,
}

#[derive(Encode, Decode)]
pub(crate) struct OldAssignment {
    pub(crate) cid_number: Cid,
    pub(crate) account_id: AccountId,
    pub(crate) role_code: RoleCode,
    pub(crate) term_start: u32,
    pub(crate) term_end: u32,
    pub(crate) assignment_source: entitybase::InstitutionAssignmentSource,
    pub(crate) assignment_source_ref: SourceRef,
    pub(crate) assignment_status: entitybase::InstitutionAssignmentStatus,
}

type OldAssignments = BoundedVec<OldAssignment, ConstU32<OLD_CAPACITY>>;

#[derive(Encode, Decode)]
pub(crate) struct OldRepresentative {
    pub(crate) family_name: Name,
    pub(crate) given_name: Name,
    pub(crate) cid_number: Cid,
    pub(crate) account_id: AccountId,
}

#[derive(Encode, Decode)]
pub(crate) struct OldInstitution {
    pub(crate) cid_full_name: Name,
    pub(crate) cid_short_name: Name,
    pub(crate) town_code: Name,
    pub(crate) legal_representative: Option<OldRepresentative>,
    pub(crate) institution_code: InstitutionCode,
    pub(crate) created_at: BlockNumber,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
pub(crate) struct Counts {
    pub(crate) public: u64,
    pub(crate) private: u64,
    pub(crate) accounts: u64,
    pub(crate) assignments: u64,
    pub(crate) missing_people: u64,
}

#[derive(Debug, Encode, Decode, Default)]
struct Progress {
    table: u8,
    cursor: Option<BoundedVec<u8, ConstU32<512>>>,
    offset: u32,
    counts: Counts,
}

// 先迁岗位和任职，再核对法定代表人；先核验账户正向记录，再搬反向索引。
const TABLES: &[&[u8]] = &[
    b"InstitutionRoles",
    b"InstitutionRoleAssignments",
    b"UsedRoleCodes",
    b"Institutions",
    b"InstitutionRolePermissions",
    b"InstitutionGovernanceThresholds",
    b"InstitutionRoleNonce",
    b"InstitutionAccounts",
    b"AccountRegisteredCid",
];
const ADMIN_TABLES: &[&[u8]] = &[
    b"AdminAccounts",
    b"AdminWallets",
    b"WalletRoleBindings",
    b":__STORAGE_VERSION__:",
];
const PURGE: u8 = (TABLES.len() * 2) as u8;
const END: u8 = PURGE + (ADMIN_TABLES.len() * 2) as u8;
const ACTIVE_PROPOSALS: &[u8] = b"ActiveProposalsBySubject";

/// 当前第一方 pallet 的前缀由各自的运行时注册名生成，旧名只用于原链读取。
pub(crate) fn renamed_pallets() -> [(&'static str, &'static str); 17] {
    [
        (
            "OnchainTransaction",
            <crate::Onchain as PalletInfoAccess>::name(),
        ),
        (
            "ProvincialBankInterest",
            <crate::PrbBankIssue as PalletInfoAccess>::name(),
        ),
        (
            "FullnodeIssuance",
            <crate::FullnodeIssue as PalletInfoAccess>::name(),
        ),
        (
            "ResolutionIssuance",
            <crate::ResolutionIssue as PalletInfoAccess>::name(),
        ),
        (
            "CitizenIdentity",
            <crate::Identity as PalletInfoAccess>::name(),
        ),
        (
            "CitizenIssuance",
            <crate::CitizenIssue as PalletInfoAccess>::name(),
        ),
        (
            "RuntimeUpgrade",
            <crate::Upgrade as PalletInfoAccess>::name(),
        ),
        (
            "ResolutionDestroy",
            <crate::Destroy as PalletInfoAccess>::name(),
        ),
        (
            "PowDifficulty",
            <crate::Difficulty as PalletInfoAccess>::name(),
        ),
        (
            "MultisigTransfer",
            <crate::Multisig as PalletInfoAccess>::name(),
        ),
        (
            "GenesisPallet",
            <crate::Genesis as PalletInfoAccess>::name(),
        ),
        (
            "OffchainTransaction",
            <crate::Offchain as PalletInfoAccess>::name(),
        ),
        (
            "OnchainIssuance",
            <crate::OnchainIssue as PalletInfoAccess>::name(),
        ),
        (
            "LegislationYuan",
            <crate::Legislation as PalletInfoAccess>::name(),
        ),
        (
            "CitizenElection",
            <crate::Election as PalletInfoAccess>::name(),
        ),
        (
            "AddressRegistry",
            <crate::Address as PalletInfoAccess>::name(),
        ),
        ("SquarePost", <crate::Square as PalletInfoAccess>::name()),
    ]
}

/// 其余升级钩子触发前完成改名，旧存储版本跟随原值搬迁。
pub struct RenamePallets;

impl OnRuntimeUpgrade for RenamePallets {
    fn on_runtime_upgrade() -> Weight {
        let result: Result<Weight, DispatchError> =
            with_transaction(|| match Self::move_prefixes() {
                Ok(weight) => TransactionOutcome::Commit(Ok(weight)),
                Err(error) => TransactionOutcome::Rollback(Err(error)),
            });
        result.expect("第一方 pallet 存储前缀迁移失败")
    }
}

impl RenamePallets {
    fn move_prefixes() -> Result<Weight, DispatchError> {
        let maximum = crate::configs::RuntimeBlockWeights::get().max_block;
        let budget = Weight::from_parts(maximum.ref_time() / 4, maximum.proof_size() / 4);
        let db = RocksDbWeight::get();
        let mut used = Weight::from_parts(0, 4_096);
        let mut rows = Vec::new();
        for (old, new) in renamed_pallets() {
            let old_prefix = sp_io::hashing::twox_128(old.as_bytes());
            let new_prefix = sp_io::hashing::twox_128(new.as_bytes());
            let mut key =
                sp_io::storage::next_key(&old_prefix).filter(|key| key.starts_with(&old_prefix));
            used = used.saturating_add(db.reads(2).saturating_add(Weight::from_parts(0, 1_024)));
            frame_support::ensure!(!sp_io::storage::exists(&old_prefix), invalid());
            if key.is_none() {
                continue;
            }
            frame_support::ensure!(
                !sp_io::storage::exists(&new_prefix)
                    && !sp_io::storage::next_key(&new_prefix)
                        .is_some_and(|next| next.starts_with(&new_prefix)),
                DispatchError::Other("pallet 迁移目标前缀已有数据"),
            );
            used = used.saturating_add(db.reads(2).saturating_add(Weight::from_parts(0, 1_024)));
            while let Some(source) = key {
                frame_support::ensure!(source.len() >= 32, invalid());
                let length = sp_io::storage::read(&source, &mut [], 0).ok_or_else(invalid)?;
                let proof = 8_192u64
                    .saturating_add(2 * u64::from(length))
                    .saturating_add(2 * source.len() as u64);
                used = used
                    .saturating_add(Weight::from_parts(20_000_000, proof))
                    .saturating_add(db.reads_writes(3, 2));
                frame_support::ensure!(
                    used.all_lte(budget),
                    DispatchError::Other("pallet 前缀迁移超出单区块预算"),
                );
                let value = sp_io::storage::get(&source).ok_or_else(invalid)?;
                let mut target = new_prefix.to_vec();
                target.extend_from_slice(&source[old_prefix.len()..]);
                key =
                    sp_io::storage::next_key(&source).filter(|next| next.starts_with(&old_prefix));
                rows.push((source, target, value));
            }
        }
        frame_support::ensure!(used.all_lte(budget), invalid());
        // 预检所有前缀和预算后统一写入，值及键后缀逐字节沿用旧状态。
        for (source, target, value) in rows {
            sp_io::storage::set(&target, &value);
            sp_io::storage::clear(&source);
        }
        Ok(used)
    }
}

const OLD_GRANDPA: &[u8] = b"GrandpaKeyChange";
const GRANDPA_TABLES: &[&[u8]] = &[
    b"CurrentGrandpaKeys",
    b"GrandpaKeyOwnerByKey",
    b"ReservedGrandpaKeys",
    b"NextGrandpaKeyProofNonce",
    b"ActiveEmergencyRecoveryByInstitution",
    b"PendingGrandpaKeyChange",
    b":__STORAGE_VERSION__:",
];
const GRANDPA_ROW_LIMIT: usize = 512;

/// 提案编号先按旧前缀扩宽，再把同一批 GRANDPA 原始状态原子交给新前缀。
pub struct GrandpaKeys;

impl GrandpaKeys {
    fn migrate() -> Result<Weight, DispatchError> {
        let old_pallet = sp_io::hashing::twox_128(OLD_GRANDPA);
        let new_pallet = sp_io::hashing::twox_128(b"GrandpaKey");
        let db = RocksDbWeight::get();
        let mut cursor = old_pallet.to_vec();
        let mut rows = Vec::new();
        let mut keys = BTreeMap::new();
        let mut owners = BTreeMap::new();
        let allowed = primitives::cid::china::china_cb::CHINA_CB;
        let registered = |cid: &Cid| {
            allowed
                .iter()
                .any(|entry| entry.cid_number.as_bytes() == cid.as_slice())
        };

        while let Some(source) =
            sp_io::storage::next_key(&cursor).filter(|key| key.starts_with(&old_pallet))
        {
            frame_support::ensure!(
                rows.len() < GRANDPA_ROW_LIMIT && source.len() >= 32,
                invalid()
            );
            let table = GRANDPA_TABLES
                .iter()
                .position(|name| source[16..32] == sp_io::hashing::twox_128(name))
                .ok_or_else(invalid)?;
            let suffix = &source[32..];
            let mut target = new_pallet.to_vec();
            target.extend_from_slice(&source[16..]);
            frame_support::ensure!(!sp_io::storage::exists(&target), invalid());
            let value = sp_io::storage::get(&source).ok_or_else(invalid)?;
            frame_support::ensure!(value.len() <= 512, invalid());

            match table {
                0 => {
                    let cid: Cid = one_key(suffix)?;
                    frame_support::ensure!(registered(&cid), invalid());
                    frame_support::ensure!(
                        keys.insert(cid, decode::<[u8; 32]>(&value)?).is_none(),
                        invalid()
                    );
                }
                1 => {
                    let key: [u8; 32] = one_key(suffix)?;
                    let cid: Cid = decode(&value)?;
                    frame_support::ensure!(registered(&cid), invalid());
                    frame_support::ensure!(owners.insert(key, cid).is_none(), invalid());
                }
                2 => {
                    let _: [u8; 32] = one_key(suffix)?;
                    frame_support::ensure!(registered(&decode::<Cid>(&value)?), invalid());
                }
                3 => {
                    frame_support::ensure!(registered(&one_key::<Cid>(suffix)?), invalid());
                    let _: u64 = decode(&value)?;
                }
                4 => {
                    frame_support::ensure!(registered(&one_key::<Cid>(suffix)?), invalid());
                    let _: u128 = decode(&value)?;
                }
                5 => {
                    frame_support::ensure!(suffix.is_empty(), invalid());
                    let pending: grandpakey::PendingGrandpaKeyChangeState<AccountId, BlockNumber> =
                        decode(&value)?;
                    frame_support::ensure!(registered(&pending.actor_cid_number), invalid());
                }
                6 => {
                    frame_support::ensure!(
                        suffix.is_empty()
                            && decode::<StorageVersion>(&value)? == StorageVersion::new(0),
                        invalid()
                    );
                }
                _ => return Err(invalid()),
            }
            cursor = source.clone();
            rows.push((source, target, value));
        }

        if rows.is_empty() {
            return Ok(db
                .reads(1)
                .saturating_add(Weight::from_parts(10_000_000, 4_096)));
        }
        frame_support::ensure!(
            !sp_io::storage::next_key(&new_pallet).is_some_and(|key| key.starts_with(&new_pallet)),
            invalid()
        );
        frame_support::ensure!(
            keys.len() == allowed.len()
                && owners.len() == allowed.len()
                && keys.iter().all(|(cid, key)| owners.get(key) == Some(cid)),
            invalid()
        );
        let count = rows.len() as u64;
        // 全部验证后才搬迁。共识集合、权重和 set id 位于独立存储中。
        for (source, target, value) in rows {
            sp_io::storage::set(&target, &value);
            sp_io::storage::clear(&source);
        }
        StorageVersion::new(1).put::<crate::GrandpaKey>();
        Ok(db
            .reads_writes(
                count.saturating_mul(3).saturating_add(2),
                count.saturating_mul(2).saturating_add(1),
            )
            .saturating_add(Weight::from_parts(
                20_000_000u64.saturating_mul(count),
                8_192u64.saturating_mul(count),
            )))
    }
}

impl OnRuntimeUpgrade for GrandpaKeys {
    fn on_runtime_upgrade() -> Weight {
        let moved: Result<Weight, DispatchError> = with_transaction(|| match Self::migrate() {
            Ok(weight) => TransactionOutcome::Commit(Ok(weight)),
            Err(error) => TransactionOutcome::Rollback(Err(error)),
        });
        moved.expect("GRANDPA 存储前缀迁移校验失败")
    }
}

pub struct Institutions;

pub(crate) fn prefix(pallet: &[u8], table: &[u8]) -> Prefix {
    frame_support::storage::storage_prefix(pallet, table)
}

fn names(public: bool) -> (&'static [u8], &'static [u8], &'static [u8]) {
    if public {
        (b"PublicManage", b"PublicEntity", b"PublicAdmins")
    } else {
        (b"PrivateManage", b"PrivateEntity", b"PrivateAdmins")
    }
}

fn invalid() -> DispatchError {
    DispatchError::Other(INVALID)
}

fn decode<T: DecodeAll>(bytes: &[u8]) -> Result<T, DispatchError> {
    T::decode_all(&mut &bytes[..]).map_err(|_| invalid())
}

fn take_key<T: Decode + Encode>(suffix: &mut &[u8]) -> Result<T, DispatchError> {
    let original = *suffix;
    let hash = original.get(..16).ok_or_else(invalid)?;
    *suffix = original.get(16..).ok_or_else(invalid)?;
    let value = T::decode(suffix).map_err(|_| invalid())?;
    frame_support::ensure!(
        sp_io::hashing::blake2_128(&value.encode()) == hash,
        invalid()
    );
    Ok(value)
}

fn one_key<T: Decode + Encode>(mut suffix: &[u8]) -> Result<T, DispatchError> {
    let value = take_key(&mut suffix)?;
    frame_support::ensure!(suffix.is_empty(), invalid());
    Ok(value)
}

fn two_keys<T: Decode + Encode, U: Decode + Encode>(
    mut suffix: &[u8],
) -> Result<(T, U), DispatchError> {
    let first = take_key(&mut suffix)?;
    let second = one_key(suffix)?;
    Ok((first, second))
}

pub(crate) fn storage_key<K: Encode>(pallet: &[u8], table: &[u8], key: &K) -> Vec<u8> {
    let mut bytes = prefix(pallet, table).to_vec();
    add_key(&mut bytes, key);
    bytes
}

pub(crate) fn add_key<K: Encode>(bytes: &mut Vec<u8>, key: &K) {
    let key = key.encode();
    bytes.extend_from_slice(&sp_io::hashing::blake2_128(&key));
    bytes.extend_from_slice(&key);
}

fn get<T: DecodeAll>(key: &[u8]) -> Result<T, DispatchError> {
    let size = sp_io::storage::read(key, &mut [], 0).ok_or_else(invalid)? as usize;
    frame_support::ensure!(size <= MAX_ROW_BYTES, invalid());
    decode(&sp_io::storage::get(key).ok_or_else(invalid)?)
}

fn next_key(prefix: &Prefix, cursor: Option<&[u8]>) -> Option<Vec<u8>> {
    // StorageVersion 是单值，键与表前缀完全相同。
    if cursor.is_none() && sp_io::storage::exists(prefix) {
        return Some(prefix.to_vec());
    }
    sp_io::storage::next_key(cursor.unwrap_or(prefix)).filter(|key| key.starts_with(prefix))
}

fn cid_code(cid: &Cid) -> Result<InstitutionCode, DispatchError> {
    let text = core::str::from_utf8(cid.as_slice()).map_err(|_| invalid())?;
    frame_support::ensure!(text.trim() == text, invalid());
    parse_cid_number_parts_bytes(cid.as_slice())
        .map(|parts| parts.institution)
        .map_err(|_| invalid())
}

fn person(wallet: &AccountId, public: bool) -> Option<Cid> {
    let cid = crate::Identity::person_cid(wallet)?;
    if public && cid_code(&cid).ok()? != *b"CTZN" {
        return None;
    }
    Some(cid)
}

/// 仅识别原链已有的五个创世岗位；机构身份沿用所属公开常量。
fn founder_role(public: bool, cid: &[u8], role: &[u8]) -> bool {
    use primitives::cid::china::{china_zf, citizenchain};
    if !public {
        return cid == citizenchain::CITIZENCHAIN_FOUNDATION.cid_number.as_bytes()
            && citizenchain::fixed_role(role).is_some();
    }
    china_zf::CHINA_ZF.iter().any(|institution| {
        institution_code_from_bytes(institution.cid_number.as_bytes())
            == Some(primitives::cid::code::FSC)
            && institution.cid_number.as_bytes() == cid
            && china_zf::FSC_FIXED_ROLES.contains(&role)
    })
}

fn institution_code_from_bytes(cid: &[u8]) -> Option<InstitutionCode> {
    parse_cid_number_parts_bytes(cid)
        .ok()
        .map(|parts| parts.institution)
}

/// 旧法代资料保留永久 CID 与原钱包；当前钱包只通过 Identity 的双向索引验证。
fn founder_person(public: bool, cid: &Cid, wallet: &AccountId) -> Result<Cid, DispatchError> {
    let info: OldInstitution = get(&storage_key(names(public).0, b"Institutions", cid))?;
    let representative = info.legal_representative.ok_or_else(invalid)?;
    frame_support::ensure!(
        info.created_at == 0
            && representative.account_id == *wallet
            && !representative.family_name.is_empty()
            && !representative.given_name.is_empty()
            && cid_code(&representative.cid_number)? == *b"CTZN"
            && crate::Identity::person_account(&representative.cid_number).is_some()
            && identity::CidRegistry::<Runtime>::get(&representative.cid_number)
                .is_some_and(|record| record.registered_at == 0),
        invalid()
    );
    Ok(representative.cid_number)
}

fn founder_assignment(public: bool, row: &OldAssignment) -> Result<Cid, DispatchError> {
    frame_support::ensure!(
        row.term_start == 0
            && row.term_end == 0
            && row.assignment_source == entitybase::InstitutionAssignmentSource::Genesis
            && row.assignment_source_ref.is_empty()
            && row.assignment_status == entitybase::InstitutionAssignmentStatus::Active,
        invalid()
    );
    founder_person(public, &row.cid_number, &row.account_id)
}

/// 写入前交叉核对两份法代资料与五条原任职，不把旧钱包的当前持有人当作原任职人。
fn check_founder() -> Result<(), DispatchError> {
    use primitives::cid::china::{china_zf, citizenchain};
    let foundation = Cid::try_from(
        citizenchain::CITIZENCHAIN_FOUNDATION
            .cid_number
            .as_bytes()
            .to_vec(),
    )
    .map_err(|_| invalid())?;
    let security = china_zf::CHINA_ZF
        .iter()
        .find(|entry| {
            institution_code_from_bytes(entry.cid_number.as_bytes())
                == Some(primitives::cid::code::FSC)
        })
        .ok_or_else(invalid)?;
    let security = Cid::try_from(security.cid_number.as_bytes().to_vec()).map_err(|_| invalid())?;
    let foundation_key = storage_key(names(false).0, b"Institutions", &foundation);
    let security_key = storage_key(names(true).0, b"Institutions", &security);
    // 不含这两机构的旧状态不产生额外任职；命中其中一个时必须同时核对另一个。
    if !sp_io::storage::exists(&foundation_key) && !sp_io::storage::exists(&security_key) {
        return Ok(());
    }
    let foundation_info: OldInstitution = get(&foundation_key)?;
    let security_info: OldInstitution = get(&security_key)?;
    let a = foundation_info.legal_representative.ok_or_else(invalid)?;
    let b = security_info.legal_representative.ok_or_else(invalid)?;
    frame_support::ensure!(
        a.cid_number == b.cid_number
            && a.account_id == b.account_id
            && a.family_name == b.family_name
            && a.given_name == b.given_name,
        invalid()
    );
    for (public, cid, roles) in [
        (
            false,
            &foundation,
            citizenchain::CITIZENCHAIN_FIXED_ROLES
                .iter()
                .map(|role| role.role_code)
                .collect::<Vec<_>>(),
        ),
        (true, &security, china_zf::FSC_FIXED_ROLES.to_vec()),
    ] {
        for raw in roles {
            let role = RoleCode::try_from(raw.to_vec()).map_err(|_| invalid())?;
            let mut role_key = storage_key(names(public).0, b"InstitutionRoles", cid);
            add_key(&mut role_key, &role);
            let definition: OldRole = get(&role_key)?;
            frame_support::ensure!(
                definition.cid_number == *cid
                    && definition.role_code == role
                    && !definition.term_required
                    && definition.role_status == entitybase::InstitutionRoleStatus::Active,
                invalid()
            );
            let mut key = storage_key(names(public).0, b"InstitutionRoleAssignments", cid);
            add_key(&mut key, &role);
            let rows: OldAssignments = get(&key)?;
            frame_support::ensure!(rows.len() == 1, invalid());
            let row = &rows[0];
            frame_support::ensure!(
                row.cid_number == *cid
                    && row.role_code == role
                    && founder_assignment(public, row)? == a.cid_number,
                invalid()
            );
        }
    }
    Ok(())
}

fn seats(cid: &Cid, role: &RoleCode, code: InstitutionCode) -> u32 {
    if role.as_slice() == ROLE_CODE_LEGAL_REPRESENTATIVE {
        return 1;
    }
    if code == primitives::cid::code::FSC && founder_role(true, cid, role) {
        return 1;
    }
    if cid.as_slice()
        == primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION
            .cid_number
            .as_bytes()
    {
        if let Some(spec) = primitives::cid::china::citizenchain::fixed_role(role.as_slice()) {
            return spec.seats;
        }
    }
    if let Some(spec) = member_composition_by_identity(code, cid.as_slice()) {
        if spec.role_code == role.as_slice() {
            return spec.max_members;
        }
    }
    fixed_role_seats_by_identity(code, cid.as_slice(), role.as_slice()).unwrap_or(OLD_CAPACITY)
}

fn convert_assignments(
    public: bool,
    suffix: &[u8],
    source: &[u8],
    target: &[u8],
    progress: &mut Progress,
) -> Result<bool, DispatchError> {
    let (cid, role_code): (Cid, RoleCode) = two_keys(suffix)?;
    let old: OldAssignments = get(source)?;
    let name = names(public).1;
    let mut role_key = storage_key(name, b"InstitutionRoles", &cid);
    add_key(&mut role_key, &role_code);
    let role: Role = get(&role_key)?;
    let offset = progress.offset as usize;
    frame_support::ensure!(
        offset <= old.len() && old.len() <= role.seats as usize,
        invalid()
    );
    let mut assigned: Assignments = if offset == 0 {
        frame_support::ensure!(!sp_io::storage::exists(target), invalid());
        Default::default()
    } else {
        get(target)?
    };
    let end = (offset + SEAT_BATCH).min(old.len());
    for row in &old[offset..end] {
        frame_support::ensure!(
            row.cid_number == cid && row.role_code == role_code,
            invalid()
        );
        let resolved = if founder_role(public, &cid, &role_code) {
            Some(founder_assignment(public, row)?)
        } else {
            person(&row.account_id, public)
        };
        let Some(person_cid) = resolved else {
            progress.counts.missing_people += 1;
            continue;
        };
        frame_support::ensure!(
            !assigned.iter().any(|item| item.person_cid == person_cid),
            invalid()
        );
        assigned
            .try_push(Assignment {
                cid_number: row.cid_number.clone(),
                person_cid,
                role_code: row.role_code.clone(),
                term_start: row.term_start,
                term_end: row.term_end,
                assignment_source: row.assignment_source,
                assignment_source_ref: row.assignment_source_ref.clone(),
                assignment_status: row.assignment_status,
            })
            .map_err(|_| invalid())?;
        progress.counts.assignments += 1;
    }
    let complete = end == old.len();
    progress.offset = if complete { 0 } else { end as u32 };
    sp_io::storage::set(target, &assigned.encode());
    Ok(complete)
}

fn check_account(
    public: bool,
    cid: &Cid,
    name: &Name,
    account: &AccountId,
) -> Result<(), DispatchError> {
    let (old, _, _) = names(public);
    let reverse: entitybase::RegisteredInstitution<Cid, Name> =
        get(&storage_key(old, b"AccountRegisteredCid", account))?;
    frame_support::ensure!(
        reverse.cid_number == *cid && reverse.account_name == *name,
        invalid()
    );
    let (old_peer, new_peer, _) = names(!public);
    for pallet in [old_peer, new_peer] {
        frame_support::ensure!(
            !sp_io::storage::exists(&storage_key(pallet, b"AccountRegisteredCid", account)),
            invalid()
        );
    }
    Ok(())
}

fn convert_institution(
    public: bool,
    cid: Cid,
    source: &[u8],
    progress: &mut Progress,
) -> Result<Vec<u8>, DispatchError> {
    let info: OldInstitution = get(source)?;
    frame_support::ensure!(cid_code(&cid)? == info.institution_code, invalid());
    let (old, new, _) = names(public);
    let (old_peer, new_peer, _) = names(!public);
    for pallet in [old_peer, new_peer] {
        frame_support::ensure!(
            !sp_io::storage::exists(&storage_key(pallet, b"Institutions", &cid)),
            invalid()
        );
    }
    let read_account = |raw: &[u8]| -> Result<AccountId, DispatchError> {
        let name = Name::try_from(raw.to_vec()).map_err(|_| invalid())?;
        let mut key = storage_key(old, b"InstitutionAccounts", &cid);
        add_key(&mut key, &name);
        let row: publicentity::InstitutionAccountInfoOf<Runtime> = get(&key)?;
        check_account(public, &cid, &name, &row.account_id)?;
        Ok(row.account_id)
    };
    frame_support::ensure!(
        read_account(RESERVED_NAME_MAIN)? != read_account(RESERVED_NAME_FEE)?,
        invalid()
    );

    let lr = RoleCode::try_from(ROLE_CODE_LEGAL_REPRESENTATIVE.to_vec()).map_err(|_| invalid())?;
    let mut role_key = storage_key(new, b"InstitutionRoles", &cid);
    add_key(&mut role_key, &lr);
    if !sp_io::storage::exists(&role_key) {
        let role = Role {
            cid_number: cid.clone(),
            role_code: lr.clone(),
            role_name: ROLE_NAME_LEGAL_REPRESENTATIVE
                .to_vec()
                .try_into()
                .map_err(|_| invalid())?,
            term_required: false,
            role_status: entitybase::InstitutionRoleStatus::Active,
            seats: 1,
        };
        sp_io::storage::set(&role_key, &role.encode());
        let mut used_key = storage_key(new, b"UsedRoleCodes", &cid);
        add_key(&mut used_key, &lr);
        sp_io::storage::set(&used_key, &true.encode());
    }
    let mut assigned_key = storage_key(new, b"InstitutionRoleAssignments", &cid);
    add_key(&mut assigned_key, &lr);
    let assigned: Assignments = if sp_io::storage::exists(&assigned_key) {
        get(&assigned_key)?
    } else {
        Default::default()
    };
    let founder = if founder_role(public, &cid, ROLE_CODE_LEGAL_REPRESENTATIVE) {
        let lr = info.legal_representative.as_ref().ok_or_else(invalid)?;
        Some(founder_person(public, &cid, &lr.account_id)?)
    } else {
        None
    };
    let legal_representative = info.legal_representative.and_then(|lr| {
        let person_cid = founder.or_else(|| person(&lr.account_id, public))?;
        if lr.cid_number != person_cid
            || lr.family_name.is_empty()
            || lr.given_name.is_empty()
            || !assigned.iter().any(|row| row.person_cid == person_cid)
        {
            return None;
        }
        Some(entitybase::LegalRepresentative {
            family_name: lr.family_name,
            given_name: lr.given_name,
            person_cid,
        })
    });
    // 资料与 LR 任职使用同一人员 CID；缺姓名或绑定证据时保留永久岗位和空缺。
    if legal_representative.is_none() && !assigned.is_empty() {
        progress.counts.missing_people += assigned.len() as u64;
        progress.counts.assignments -= assigned.len() as u64;
        sp_io::storage::clear(&assigned_key);
    }
    if public {
        progress.counts.public += 1;
    } else {
        progress.counts.private += 1;
    }
    Ok(entitybase::InstitutionInfo {
        cid_full_name: info.cid_full_name,
        cid_short_name: info.cid_short_name,
        town_code: info.town_code,
        legal_representative,
        institution_code: info.institution_code,
        created_at: info.created_at,
    }
    .encode())
}

fn convert(
    public: bool,
    table: &[u8],
    suffix: &[u8],
    source: &[u8],
    progress: &mut Progress,
) -> Result<Vec<u8>, DispatchError> {
    let (_, new, _) = names(public);
    let value = match table {
        b"InstitutionRoles" => {
            let (cid, role_code): (Cid, RoleCode) = two_keys(suffix)?;
            let role: OldRole = get(source)?;
            frame_support::ensure!(
                role.cid_number == cid && role.role_code == role_code,
                invalid()
            );
            let capacity = seats(&cid, &role_code, cid_code(&cid)?);
            Role {
                cid_number: cid,
                role_code,
                role_name: role.role_name,
                term_required: role.term_required,
                role_status: role.role_status,
                seats: capacity,
            }
            .encode()
        }
        b"Institutions" => convert_institution(public, one_key(suffix)?, source, progress)?,
        b"InstitutionAccounts" => {
            let (cid, name): (Cid, Name) = two_keys(suffix)?;
            let row: publicentity::InstitutionAccountInfoOf<Runtime> = get(source)?;
            check_account(public, &cid, &name, &row.account_id)?;
            frame_support::ensure!(
                sp_io::storage::exists(&storage_key(new, b"Institutions", &cid)),
                invalid()
            );
            progress.counts.accounts += 1;
            row.encode()
        }
        b"AccountRegisteredCid" => {
            let account: AccountId = one_key(suffix)?;
            let row: entitybase::RegisteredInstitution<Cid, Name> = get(source)?;
            let mut key = storage_key(new, b"InstitutionAccounts", &row.cid_number);
            add_key(&mut key, &row.account_name);
            let forward: publicentity::InstitutionAccountInfoOf<Runtime> = get(&key)?;
            frame_support::ensure!(forward.account_id == account, invalid());
            row.encode()
        }
        b"InstitutionRolePermissions" => {
            let (cid, code): (Cid, RoleCode) = two_keys(suffix)?;
            let permissions: publicentity::RolePermissionsOf<Runtime> = get(source)?;
            frame_support::ensure!(
                permissions.iter().all(|p| {
                    p.role_subject.cid_number == cid && p.role_subject.role_code == code
                }),
                invalid()
            );
            permissions.encode()
        }
        b"InstitutionGovernanceThresholds" => {
            let cid: Cid = one_key(suffix)?;
            frame_support::ensure!(
                sp_io::storage::exists(&storage_key(new, b"Institutions", &cid)),
                invalid()
            );
            get::<u32>(source)?.encode()
        }
        b"InstitutionRoleNonce" => {
            let _: Cid = one_key(suffix)?;
            get::<u64>(source)?.encode()
        }
        b"UsedRoleCodes" => {
            let _: (Cid, RoleCode) = two_keys(suffix)?;
            get::<bool>(source)?.encode()
        }
        _ => return Err(invalid()),
    };
    Ok(value)
}

impl Institutions {
    pub(crate) fn pending() -> bool {
        sp_io::storage::exists(PROGRESS)
    }

    pub(crate) fn legacy() -> bool {
        [true, false].into_iter().any(|public| {
            let (old, _, _) = names(public);
            next_key(&prefix(old, b"Institutions"), None).is_some()
        })
    }

    fn preflight() -> Result<(), DispatchError> {
        check_founder()?;
        // 旧活跃提案和待执行机构操作只能在旧版本完成。检查在任何迁移写入前执行。
        let active_prefix = prefix(b"VotingEngine", ACTIVE_PROPOSALS);
        let mut cursor = None;
        for _ in 0..ACTIVE_CHECK_LIMIT {
            let Some(key) = next_key(&active_prefix, cursor.as_deref()) else {
                break;
            };
            // 既有引擎会给已结案主体保留 SCALE 空向量，不构成活跃提案。
            frame_support::ensure!(
                sp_io::storage::get(&key).is_some_and(|bytes| bytes.as_ref() == [0u8]),
                DispatchError::Other("旧链仍有活跃提案")
            );
            cursor = Some(key);
        }
        frame_support::ensure!(
            next_key(&active_prefix, cursor.as_deref()).is_none(),
            DispatchError::Other("活跃提案索引预检超过有界预算")
        );
        for public in [true, false] {
            let (old, new, _) = names(public);
            for table in [
                b"InstitutionPendingClose".as_slice(),
                b"InstitutionPendingAdd",
            ] {
                frame_support::ensure!(next_key(&prefix(old, table), None).is_none(), invalid());
            }
            for table in TABLES {
                frame_support::ensure!(next_key(&prefix(new, table), None).is_none(), invalid());
            }
        }
        Ok(())
    }

    fn run_row(progress: &mut Progress) -> Result<(), DispatchError> {
        let table = progress.table;
        let cursor = progress.cursor.as_ref().map(|key| key.as_slice());
        let (from, to) = if table < PURGE {
            let (old, new, _) = names(table % 2 == 0);
            let table_name = TABLES[usize::from(table / 2)];
            (prefix(old, table_name), Some(prefix(new, table_name)))
        } else {
            let index = table - PURGE;
            let (_, _, old) = names(index % 2 == 0);
            (prefix(old, ADMIN_TABLES[usize::from(index / 2)]), None)
        };

        let Some(key) = next_key(&from, cursor) else {
            frame_support::ensure!(progress.offset == 0, invalid());
            progress.table += 1;
            progress.cursor = None;
            return Ok(());
        };
        let suffix = &key[from.len()..];
        if let Some(to) = to {
            let mut target = to.to_vec();
            target.extend_from_slice(suffix);
            let table_name = TABLES[usize::from(table / 2)];
            if table_name == b"InstitutionRoleAssignments" {
                if !convert_assignments(table % 2 == 0, suffix, &key, &target, progress)? {
                    return Ok(());
                }
            } else {
                frame_support::ensure!(!sp_io::storage::exists(&target), invalid());
                let value = convert(table % 2 == 0, table_name, suffix, &key, progress)?;
                sp_io::storage::set(&target, &value);
            }
            sp_io::storage::clear(&key);
        } else {
            sp_io::storage::clear(&key);
        }
        progress.cursor = Some(key.try_into().map_err(|_| invalid())?);
        Ok(())
    }

    fn finish(mut progress: Progress) -> Result<(), DispatchError> {
        for public in [true, false] {
            let (old, _, admin) = names(public);
            // 每个已知条目已完成；未知旧键留作失败证据，禁止整前缀盲删。
            let version = prefix(old, b":__STORAGE_VERSION__:");
            sp_io::storage::clear(&version);
            let mut old_prefix = sp_io::hashing::twox_128(old).to_vec();
            frame_support::ensure!(
                !sp_io::storage::next_key(&old_prefix).is_some_and(|k| k.starts_with(&old_prefix)),
                invalid()
            );
            old_prefix = sp_io::hashing::twox_128(admin).to_vec();
            frame_support::ensure!(
                !sp_io::storage::next_key(&old_prefix).is_some_and(|k| k.starts_with(&old_prefix)),
                invalid()
            );
        }
        Self::add_founder_assignments(&mut progress.counts)?;
        StorageVersion::new(VERSION).put::<PublicEntity>();
        StorageVersion::new(VERSION).put::<PrivateEntity>();
        unhashed::put(RESULT, &progress.counts);
        unhashed::kill(PROGRESS);
        Ok(())
    }

    /// 本次升级只补两条普通创世任职；人员与钱包均复用迁移后的链上真源。
    fn add_founder_assignments(counts: &mut Counts) -> Result<(), DispatchError> {
        use primitives::cid::china::{china_cb::CHINA_CB, citizenchain};
        use primitives::governance_skeleton::{
            federal_registry_institution, province_commissioner_role_code,
            ROLE_CODE_COMMITTEE_MEMBER,
        };
        let foundation = Cid::try_from(
            citizenchain::CITIZENCHAIN_FOUNDATION
                .cid_number
                .as_bytes()
                .to_vec(),
        )
        .map_err(|_| invalid())?;
        let key = storage_key(names(false).1, b"Institutions", &foundation);
        if !sp_io::storage::exists(&key) {
            return Ok(());
        }
        let info: entitybase::InstitutionInfo<BlockNumber, Name, Cid> = get(&key)?;
        let representative = info.legal_representative.ok_or_else(invalid)?;
        let person_cid = representative.person_cid;
        frame_support::ensure!(
            cid_code(&person_cid)? == *b"CTZN"
                && crate::Identity::person_account(&person_cid).is_some(),
            invalid()
        );
        let nrc = CHINA_CB
            .iter()
            .find(|entry| {
                institution_code_from_bytes(entry.cid_number.as_bytes())
                    == Some(primitives::cid::code::NRC)
            })
            .ok_or_else(invalid)?;
        for (raw_cid, raw_role) in [
            (nrc.cid_number, ROLE_CODE_COMMITTEE_MEMBER.to_vec()),
            (
                federal_registry_institution().cid_number,
                province_commissioner_role_code(*b"GZ"),
            ),
        ] {
            let cid = Cid::try_from(raw_cid.as_bytes().to_vec()).map_err(|_| invalid())?;
            let role_code = RoleCode::try_from(raw_role).map_err(|_| invalid())?;
            let institution: entitybase::InstitutionInfo<BlockNumber, Name, Cid> =
                get(&storage_key(names(true).1, b"Institutions", &cid))?;
            let mut key = storage_key(names(true).1, b"InstitutionRoles", &cid);
            add_key(&mut key, &role_code);
            let role: Role = get(&key)?;
            frame_support::ensure!(
                role.cid_number == cid
                    && role.role_code == role_code
                    && !role.term_required
                    && role.role_status == entitybase::InstitutionRoleStatus::Active
                    && fixed_role_seats_by_identity(institution.institution_code, &cid, &role_code)
                        == Some(role.seats),
                invalid()
            );
            let mut key = storage_key(names(true).1, b"InstitutionRoleAssignments", &cid);
            add_key(&mut key, &role_code);
            let mut rows: Assignments = if sp_io::storage::exists(&key) {
                get(&key)?
            } else {
                Default::default()
            };
            frame_support::ensure!(
                rows.iter()
                    .all(|row| row.cid_number == cid && row.role_code == role_code)
                    && rows.len() < role.seats as usize
                    && !rows.iter().any(|row| row.person_cid == person_cid),
                invalid()
            );
            // 记录真实迁移区块及本链身份，不伪造区块0已有任职或另存钱包。
            let source_ref = (
                frame_system::Pallet::<Runtime>::block_hash(0),
                frame_system::Pallet::<Runtime>::block_number(),
            )
                .encode();
            rows.try_push(Assignment {
                cid_number: cid,
                person_cid: person_cid.clone(),
                role_code,
                term_start: 0,
                term_end: 0,
                assignment_source: entitybase::InstitutionAssignmentSource::Genesis,
                assignment_source_ref: source_ref.try_into().map_err(|_| invalid())?,
                assignment_status: entitybase::InstitutionAssignmentStatus::Active,
            })
            .map_err(|_| invalid())?;
            sp_io::storage::set(&key, &rows.encode());
            counts.assignments += 1;
        }
        Ok(())
    }

    fn row_weight(progress: &Progress, remaining: Weight) -> Option<Weight> {
        let table = progress.table;
        let db = RocksDbWeight::get();
        if table == 2 || table == 3 {
            // SCALE 向量头给出待核验人数。当键较大时预留一整次有界读取的预算。
            let probe =
                Weight::from_parts(900_000_000, 1_650_000).saturating_add(db.reads_writes(120, 4));
            if !probe.all_lte(remaining) {
                return None;
            }
            let (old, _, _) = names(table % 2 == 0);
            let source = prefix(old, b"InstitutionRoleAssignments");
            let cursor = progress.cursor.as_ref().map(|key| key.as_slice());
            let Some(key) = next_key(&source, cursor) else {
                return Some(Weight::from_parts(0, 8_192).saturating_add(db.reads(2)));
            };
            let mut head = [0; 5];
            let Some(size) = sp_io::storage::read(&key, &mut head, 0) else {
                return Some(probe);
            };
            let Ok(codec::Compact(count)) = codec::Compact::<u32>::decode(&mut head.as_slice())
            else {
                return Some(probe);
            };
            if size as usize > MAX_LEGACY_ASSIGNMENT_BYTES || count > OLD_CAPACITY {
                return Some(probe);
            }
            let people = u64::from(count.saturating_sub(progress.offset).min(SEAT_BATCH as u32));
            return Some(
                Weight::from_parts(
                    150_000_000u64.saturating_add(20_000_000 * people),
                    22_000u64
                        .saturating_add(u64::from(size) * 2)
                        .saturating_add(u64::from(progress.offset) * 32)
                        .saturating_add(15_000 * people),
                )
                .saturating_add(db.reads_writes(20 + 4 * people, 4)),
            );
        }
        Some(if table == END {
            // 完成时预留原有 PRC 权限补齐的固定名单读取和写入。
            Weight::from_parts(600_000_000, 1_200_000).saturating_add(db.reads_writes(128, 64))
        } else if table >= PURGE {
            Weight::from_parts(120_000_000, 64_000).saturating_add(db.reads_writes(2, 2))
        } else {
            let (proof, reads) = match TABLES[usize::from(table / 2)] {
                b"Institutions" => (65_536, 22),
                b"InstitutionRolePermissions" => (52_000, 7),
                b"InstitutionAccounts" => (32_000, 13),
                b"AccountRegisteredCid" => (20_000, 9),
                _ => (16_000, 8),
            };
            Weight::from_parts(240_000_000, proof).saturating_add(db.reads_writes(reads, 6))
        })
    }

    pub(crate) fn step_with_budget(budget: Weight) -> Weight {
        let db = RocksDbWeight::get();
        let base = Weight::from_parts(0, 4_096).saturating_add(db.reads_writes(1, 1));
        if !base.all_lte(budget) {
            return Weight::zero();
        }
        let Some(mut progress) = unhashed::get::<Progress>(PROGRESS) else {
            return db.reads(1);
        };
        let mut used = base;
        for _ in 0..BATCH_LIMIT {
            let Some(cost) = Self::row_weight(&progress, budget.saturating_sub(used)) else {
                break;
            };
            if !used.saturating_add(cost).all_lte(budget) {
                break;
            }
            used = used.saturating_add(cost);
            let done = progress.table == END;
            let result: Result<(), DispatchError> = with_transaction(|| {
                let step = if done {
                    let state = core::mem::take(&mut progress);
                    Self::finish(state)
                } else {
                    Self::run_row(&mut progress).map(|()| unhashed::put(PROGRESS, &progress))
                };
                match step {
                    Ok(()) => TransactionOutcome::Commit(Ok(())),
                    Err(error) => TransactionOutcome::Rollback(Err(error)),
                }
            });
            if result.is_err() {
                sp_runtime::print(INVALID);
                // 游标和原始证据保留在失败批次之前。升级验收必须先用最终状态排演。
                break;
            }
            if done {
                crate::configs::AddPrcFeeSweepPermissions::on_runtime_upgrade();
                break;
            }
        }
        used
    }
}

impl OnRuntimeUpgrade for Institutions {
    fn on_runtime_upgrade() -> Weight {
        let db = RocksDbWeight::get();
        if Self::pending() || !Self::legacy() {
            return db.reads(5);
        }
        // 新 Runtime 启动前即核验旧链已处于可迁移状态，避免旧活动提案被新逻辑消费。
        Self::preflight().expect("机构迁移前置检查失败");
        unhashed::put(PROGRESS, &Progress::default());
        db.reads_writes(48 + u64::from(ACTIVE_CHECK_LIMIT) * 2, 1)
    }
}

impl MultiStepMigrator for Institutions {
    fn ongoing() -> bool {
        Self::pending()
    }

    fn step() -> Weight {
        let max = crate::configs::RuntimeBlockWeights::get().max_block;
        Self::step_with_budget(Weight::from_parts(max.ref_time() / 4, max.proof_size() / 4))
    }
}

// EVM 账户迁移扫描实体前缀。机构迁移完成前保留原游标，按既有钩子随后继续。
const EVM_DEFERRED: &[u8] = b":citizenchain:entity-migration:evm-deferred:v1";
pub struct EvmBackfill;

impl OnRuntimeUpgrade for EvmBackfill {
    fn on_runtime_upgrade() -> Weight {
        if Institutions::pending() {
            unhashed::put(EVM_DEFERRED, &true);
            return RocksDbWeight::get().reads_writes(1, 1);
        }
        crate::evm_accounts::BackfillEvmAccounts::on_runtime_upgrade()
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
        if Institutions::pending() || Institutions::legacy() {
            return Ok(None::<Vec<u8>>.encode());
        }
        let state = crate::evm_accounts::BackfillEvmAccounts::pre_upgrade()?;
        Ok(Some(state).encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
        let state = Option::<Vec<u8>>::decode_all(&mut state.as_slice())
            .map_err(|_| "EVM 回填预检状态无效")?;
        match state {
            Some(state) => crate::evm_accounts::BackfillEvmAccounts::post_upgrade(state),
            None if sp_io::storage::exists(EVM_DEFERRED) => Ok(()),
            None => Err("EVM 回填尚未安排到机构迁移之后".into()),
        }
    }
}

impl frame_support::traits::PostInherents for EvmBackfill {
    fn post_inherents() {
        let overhead = RocksDbWeight::get().reads(2);
        crate::System::register_extra_weight_unchecked(
            overhead,
            frame_support::dispatch::DispatchClass::Mandatory,
        );
        if Institutions::pending() {
            return;
        }
        if sp_io::storage::exists(EVM_DEFERRED) {
            unhashed::kill(EVM_DEFERRED);
            let used = crate::evm_accounts::BackfillEvmAccounts::on_runtime_upgrade()
                .saturating_add(RocksDbWeight::get().reads_writes(2, 1));
            crate::System::register_extra_weight_unchecked(
                used,
                frame_support::dispatch::DispatchClass::Mandatory,
            );
        } else {
            <crate::evm_accounts::BackfillEvmAccounts as frame_support::traits::PostInherents>::post_inherents();
        }
    }
}

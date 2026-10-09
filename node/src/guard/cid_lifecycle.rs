//! CID、机构身份与机构账户完整性节点永久策略。
//!
//! 机构唯一主键是 CID；`Institutions[cid_number]` 保存机构身份，
//! `InstitutionAccounts[(cid_number, account_name)]` 保存账户正向真源，
//! `AccountRegisteredCid[account]` 只作反向索引。主账户、费用账户和制度专属账户
//! 都只是协议账户，不承担机构身份、管理员根或生命周期状态。

use std::collections::{BTreeMap, BTreeSet};

use codec::{Decode, Encode};
use sp_crypto_hashing::blake2_128;

use primitives::account_derive::{
    institution_kind_by_name, institution_protocol_account_name, institution_protocol_kind_by_name,
    InstitutionProtocolAccountKind,
};
use primitives::cid::code::{
    institution_code_from_str, is_fixed_governance_code, is_person_code, is_private_legal_code,
    is_public_legal_code, is_three_char_code, is_unincorporated_code, profit_policy,
    province_code_text, InstitutionCode, ProfitPolicy,
};
use primitives::core_const::SS58_FORMAT;

const CITIZEN_IDENTITY_PALLET: &[u8] = b"CitizenIdentity";
const PUBLIC_MANAGE_PALLET: &[u8] = b"PublicManage";
const PRIVATE_MANAGE_PALLET: &[u8] = b"PrivateManage";
const CODE_KEY: &[u8] = b":code";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Namespace {
    Public,
    Private,
}

impl Namespace {
    fn pallet(self) -> &'static [u8] {
        match self {
            Self::Public => PUBLIC_MANAGE_PALLET,
            Self::Private => PRIVATE_MANAGE_PALLET,
        }
    }

    fn sibling(self) -> Self {
        match self {
            Self::Public => Self::Private,
            Self::Private => Self::Public,
        }
    }

    fn id(self) -> u8 {
        match self {
            Self::Public => 0,
            Self::Private => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, PartialEq)]
enum CitizenCidStatus {
    Active,
    Revoked,
}

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, PartialEq)]
enum CitizenStatus {
    Normal,
    Revoked,
}

/// 公民投票身份以永久 CID 为主键；账户仅通过独立双向索引表达当前签名绑定。
#[derive(Clone, Debug, Decode, Encode, Eq, PartialEq)]
struct CitizenVotingIdentity {
    passport_valid_from: u32,
    passport_valid_until: u32,
    citizen_status: CitizenStatus,
    residence_province_code: Vec<u8>,
    residence_city_code: Vec<u8>,
    residence_town_code: Vec<u8>,
    updated_at: u32,
}

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, PartialEq)]
enum CitizenSex {
    Male,
    Female,
}

/// 竞选身份同样以永久 CID 为主键；当前账户不进入档案，也不参与字段连续性。
#[derive(Clone, Debug, Decode, Encode, Eq, PartialEq)]
struct CitizenCandidateIdentity {
    birth_province_code: Vec<u8>,
    birth_city_code: Vec<u8>,
    birth_town_code: Vec<u8>,
    family_name: Vec<u8>,
    given_name: Vec<u8>,
    citizen_sex: CitizenSex,
    birth_date: u32,
    updated_at: u32,
}

#[derive(Clone, Debug, Decode, Encode, Eq, PartialEq)]
struct CitizenCidRecord {
    registrar_cid_number: Vec<u8>,
    commitment: [u8; 32],
    residence_province_code: Vec<u8>,
    residence_city_code: Vec<u8>,
    status: CitizenCidStatus,
    registered_at: u32,
    revoked_at: Option<u32>,
}

#[derive(Clone, Debug, Decode, Encode, Eq, PartialEq)]
struct InstitutionRecord {
    cid_full_name: Vec<u8>,
    cid_short_name: Vec<u8>,
    town_code: Vec<u8>,
    legal_representative: Option<InstitutionLegalRepresentative>,
    institution_code: InstitutionCode,
    created_at: u32,
}

#[derive(Clone, Debug, Decode, Encode, Eq, PartialEq)]
struct InstitutionLegalRepresentative {
    family_name: Vec<u8>,
    given_name: Vec<u8>,
    cid_number: Vec<u8>,
    account: [u8; 32],
}

#[derive(Clone, Debug, Decode, Encode, Eq, PartialEq)]
struct RegisteredInstitution {
    cid_number: Vec<u8>,
    account_name: Vec<u8>,
}

#[derive(Clone, Debug, Decode, Encode, Eq, PartialEq)]
struct InstitutionAccountRecord {
    address: [u8; 32],
    initial_balance: u128,
    created_at: u32,
}

/// 创世机构只冻结不可替换的身份字段；名称和法定代表人仍由 runtime 业务管理。
#[derive(Clone, Debug, Eq, PartialEq)]
struct GenesisInstitutionIdentity {
    institution_code: InstitutionCode,
    town_code: Vec<u8>,
    created_at: u32,
}

/// block#0 的机构身份基准。
///
/// 普通机构不进入本集合，后续可以依法修改或删除；节点只永久保护创世机构本身不被
/// 删除、跨命名空间复制或替换成另一主体。
#[derive(Clone, Debug, Default)]
pub struct GenesisReference {
    institutions: BTreeMap<(u8, Vec<u8>), GenesisInstitutionIdentity>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum GuardError {
    StorageKeyMalformed(&'static str),
    StorageValueDecodeFailed(&'static str),
    InvalidCid,
    InvalidCidNamespace,
    CrossNamespaceDuplicate,
    CitizenCidDeleted,
    CitizenCidIdentityChanged,
    CitizenCidStatusInvalid,
    CitizenCidRevocationHeightInvalid,
    CitizenIdentityDeleted,
    CitizenIdentityWithoutCid,
    CitizenCandidateIdentityDeleted,
    CitizenCandidateIdentityWithoutVotingIdentity,
    CitizenCandidateIdentityInvalidStatus,
    CitizenCandidateIdentityLockedFieldChanged,
    CitizenCandidateIdentityUpdateHeightInvalid,
    CitizenAccountIdBindingMissing,
    CitizenAccountIdBindingMismatch,
    CitizenBindingRevisionMissing,
    CitizenBindingRevisionInvalid,
    CitizenBindingRevisionTransitionInvalid,
    GenesisInstitutionDeleted,
    GenesisInstitutionChanged,
    InstitutionIdentityChanged,
    InstitutionLegalRepresentativeInvalid,
    FixedInstitutionCreatedAfterGenesis,
    AccountChanged,
    ProtocolAccountDeleted,
    AccountWithoutInstitution,
    AccountAddressMismatch,
    AccountReverseIndexMissing,
    AccountReverseIndexMismatch,
    RequiredProtocolAccountMissing,
    UnexpectedProtocolAccount,
    SingletonInstitutionMissing,
    SingletonInstitutionIdentityMismatch,
}

pub mod storage_key {
    use super::*;

    fn storage_prefix(pallet: &[u8], storage: &[u8]) -> Vec<u8> {
        crate::shared::storage_keys::prefix(pallet, storage)
    }

    fn map_vec(pallet: &[u8], storage: &[u8], value: &[u8]) -> Vec<u8> {
        crate::shared::storage_keys::blake2_map(pallet, storage, &value.to_vec().encode())
    }

    fn map_account(pallet: &[u8], storage: &[u8], account: &[u8; 32]) -> Vec<u8> {
        crate::shared::storage_keys::blake2_map(pallet, storage, account)
    }

    fn double_map_vec(pallet: &[u8], storage: &[u8], first: &[u8], second: &[u8]) -> Vec<u8> {
        crate::shared::storage_keys::blake2_double_map(
            pallet,
            storage,
            &first.to_vec().encode(),
            &second.to_vec().encode(),
        )
    }

    pub fn citizen_registry_prefix() -> Vec<u8> {
        storage_prefix(CITIZEN_IDENTITY_PALLET, b"CidRegistry")
    }

    pub fn citizen_registry(cid: &[u8]) -> Vec<u8> {
        map_vec(CITIZEN_IDENTITY_PALLET, b"CidRegistry", cid)
    }

    pub fn voting_identity_prefix() -> Vec<u8> {
        storage_prefix(CITIZEN_IDENTITY_PALLET, b"VotingIdentityByCid")
    }

    pub fn voting_identity(cid: &[u8]) -> Vec<u8> {
        map_vec(CITIZEN_IDENTITY_PALLET, b"VotingIdentityByCid", cid)
    }

    pub fn candidate_identity_prefix() -> Vec<u8> {
        storage_prefix(CITIZEN_IDENTITY_PALLET, b"CandidateIdentityByCid")
    }

    pub fn candidate_identity(cid: &[u8]) -> Vec<u8> {
        map_vec(CITIZEN_IDENTITY_PALLET, b"CandidateIdentityByCid", cid)
    }

    pub fn account_id_by_cid_prefix() -> Vec<u8> {
        storage_prefix(CITIZEN_IDENTITY_PALLET, b"AccountIdByCid")
    }

    pub fn account_id_by_cid(cid: &[u8]) -> Vec<u8> {
        map_vec(CITIZEN_IDENTITY_PALLET, b"AccountIdByCid", cid)
    }

    pub fn cid_by_account_id_prefix() -> Vec<u8> {
        storage_prefix(CITIZEN_IDENTITY_PALLET, b"CidByAccountId")
    }

    pub fn cid_by_account_id(account: &[u8; 32]) -> Vec<u8> {
        map_account(CITIZEN_IDENTITY_PALLET, b"CidByAccountId", account)
    }

    pub fn binding_revision_by_cid_prefix() -> Vec<u8> {
        storage_prefix(CITIZEN_IDENTITY_PALLET, b"BindingRevisionByCid")
    }

    pub fn binding_revision_by_cid(cid: &[u8]) -> Vec<u8> {
        map_vec(CITIZEN_IDENTITY_PALLET, b"BindingRevisionByCid", cid)
    }

    pub fn institution_prefix(namespace: Namespace) -> Vec<u8> {
        storage_prefix(namespace.pallet(), b"Institutions")
    }

    pub fn institution(namespace: Namespace, cid: &[u8]) -> Vec<u8> {
        map_vec(namespace.pallet(), b"Institutions", cid)
    }

    pub fn institution_account_prefix(namespace: Namespace) -> Vec<u8> {
        storage_prefix(namespace.pallet(), b"InstitutionAccounts")
    }

    pub fn institution_account_id(namespace: Namespace, cid: &[u8], name: &[u8]) -> Vec<u8> {
        double_map_vec(namespace.pallet(), b"InstitutionAccounts", cid, name)
    }

    pub fn account_registered_prefix(namespace: Namespace) -> Vec<u8> {
        storage_prefix(namespace.pallet(), b"AccountRegisteredCid")
    }

    pub fn account_registered(namespace: Namespace, account: &[u8; 32]) -> Vec<u8> {
        map_account(namespace.pallet(), b"AccountRegisteredCid", account)
    }

    /// 启动、runtime 升级和 block#0 状态检查枚举的全部 CID 规范表。
    pub fn enumerated_prefixes() -> Vec<Vec<u8>> {
        vec![
            citizen_registry_prefix(),
            voting_identity_prefix(),
            candidate_identity_prefix(),
            account_id_by_cid_prefix(),
            cid_by_account_id_prefix(),
            binding_revision_by_cid_prefix(),
            institution_prefix(Namespace::Public),
            institution_prefix(Namespace::Private),
            institution_account_prefix(Namespace::Public),
            institution_account_prefix(Namespace::Private),
            account_registered_prefix(Namespace::Public),
            account_registered_prefix(Namespace::Private),
        ]
    }

    pub fn relevant_prefixes() -> Vec<Vec<u8>> {
        enumerated_prefixes()
    }
}

fn decode_exact<T: Decode>(raw: &[u8], label: &'static str) -> Result<T, GuardError> {
    let mut input = raw;
    let value = T::decode(&mut input).map_err(|_| GuardError::StorageValueDecodeFailed(label))?;
    if !input.is_empty() {
        return Err(GuardError::StorageValueDecodeFailed(label));
    }
    Ok(value)
}

fn parse_vec_map_key(
    key: &[u8],
    prefix: &[u8],
    label: &'static str,
) -> Result<Vec<u8>, GuardError> {
    if !key.starts_with(prefix) || key.len() < prefix.len() + 17 {
        return Err(GuardError::StorageKeyMalformed(label));
    }
    let hash_at = prefix.len();
    let encoded_at = hash_at + 16;
    let encoded = &key[encoded_at..];
    if blake2_128(encoded) != key[hash_at..encoded_at] {
        return Err(GuardError::StorageKeyMalformed(label));
    }
    decode_exact(encoded, label)
}

fn parse_account_map_key(
    key: &[u8],
    prefix: &[u8],
    label: &'static str,
) -> Result<[u8; 32], GuardError> {
    if !key.starts_with(prefix) || key.len() != prefix.len() + 16 + 32 {
        return Err(GuardError::StorageKeyMalformed(label));
    }
    let hash_at = prefix.len();
    let account_at = hash_at + 16;
    if blake2_128(&key[account_at..]) != key[hash_at..account_at] {
        return Err(GuardError::StorageKeyMalformed(label));
    }
    key[account_at..]
        .try_into()
        .map_err(|_| GuardError::StorageKeyMalformed(label))
}

fn parse_double_vec_key(
    key: &[u8],
    prefix: &[u8],
    label: &'static str,
) -> Result<(Vec<u8>, Vec<u8>), GuardError> {
    if !key.starts_with(prefix) || key.len() < prefix.len() + 34 {
        return Err(GuardError::StorageKeyMalformed(label));
    }
    let first_hash_at = prefix.len();
    let first_encoded_at = first_hash_at + 16;
    let first_encoded = &key[first_encoded_at..];
    let mut input = first_encoded;
    let first =
        Vec::<u8>::decode(&mut input).map_err(|_| GuardError::StorageKeyMalformed(label))?;
    let first_encoded_len = first_encoded.len() - input.len();
    if blake2_128(&first_encoded[..first_encoded_len]) != key[first_hash_at..first_encoded_at] {
        return Err(GuardError::StorageKeyMalformed(label));
    }
    let second_hash_at = first_encoded_at + first_encoded_len;
    if key.len() < second_hash_at + 17 {
        return Err(GuardError::StorageKeyMalformed(label));
    }
    let second_encoded_at = second_hash_at + 16;
    let second_encoded = &key[second_encoded_at..];
    if blake2_128(second_encoded) != key[second_hash_at..second_encoded_at] {
        return Err(GuardError::StorageKeyMalformed(label));
    }
    let second = decode_exact(second_encoded, label)?;
    Ok((first, second))
}

fn checksum_value(byte: u8) -> usize {
    match byte {
        b'0'..=b'9' => usize::from(byte - b'0'),
        b'A'..=b'Z' => usize::from(byte - b'A') + 10,
        _ => 0,
    }
}

fn checksum_acc(parts: &[&[u8]]) -> usize {
    let mut index = 0usize;
    let mut total = 0usize;
    for byte in parts.iter().flat_map(|part| part.iter().copied()) {
        index = index.saturating_add(1);
        total = total.wrapping_add(index.wrapping_mul(checksum_value(byte)));
    }
    total
}

fn valid_province_city(code: InstitutionCode, r5: &[u8]) -> bool {
    // 人主体(公民/居民/智能人)去地域化:R5 = CN 国家码 + 号码高 3 位(数字),不承载省/市码。
    if is_person_code(&code) {
        return r5.len() == 5 && &r5[..2] == b"CN" && r5[2..].iter().all(u8::is_ascii_digit);
    }
    // 机构:省码 + 市码必须都命中区划真源。
    let Ok(province): Result<[u8; 2], _> = r5[..2].try_into() else {
        return false;
    };
    if province_code_text(&province).is_none() {
        return false;
    }
    let city = &r5[2..];
    let mut found = false;
    primitives::cid::china::area::for_each_area(|item| {
        if let primitives::cid::china::area::AreaItem::City(candidate) = item {
            if candidate.province_code.as_bytes() == province
                && candidate.city_code.as_bytes() == city
            {
                found = true;
            }
        }
    });
    found
}

/// 节点只背书省市码、机构码、盈利属性和校验码四项制度规则。
///
/// 中间随机段和末尾年份段只作为校验码原始载荷使用，本函数不判断其长度、字符、
/// 随机派生值或具体年份。
fn parse_cid(cid: &[u8]) -> Result<InstitutionCode, GuardError> {
    let segments = cid.split(|byte| *byte == b'-').collect::<Vec<_>>();
    if segments.len() != 4 || segments[0].len() != 5 || segments[1].len() != 5 {
        return Err(GuardError::InvalidCid);
    }
    let r5 = segments[0];
    let core = segments[1];
    if !r5.iter().all(u8::is_ascii_alphanumeric)
        || !core.iter().all(u8::is_ascii_alphanumeric)
        || r5.iter().any(u8::is_ascii_lowercase)
        || core.iter().any(u8::is_ascii_lowercase)
    {
        return Err(GuardError::InvalidCid);
    }

    let (code_text, profit, checksum, expected_checksum) = if core[3].is_ascii_digit() {
        let code_text = core::str::from_utf8(&core[..3]).map_err(|_| GuardError::InvalidCid)?;
        let profit = match core[3] {
            b'0' => false,
            b'1' => true,
            _ => return Err(GuardError::InvalidCid),
        };
        let total = checksum_acc(&[r5, &core[..4], segments[2], segments[3]]);
        let expected = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"[total % 36];
        (code_text, profit, core[4], expected)
    } else {
        let code_text = core::str::from_utf8(&core[..4]).map_err(|_| GuardError::InvalidCid)?;
        let profit = core[4].is_ascii_digit();
        if !profit && !core[4].is_ascii_uppercase() {
            return Err(GuardError::InvalidCid);
        }
        let total = checksum_acc(&[r5, &core[..4], segments[2], segments[3]]);
        let expected = if profit {
            b'0' + (total % 10) as u8
        } else {
            b'A' + (total % 26) as u8
        };
        (code_text, profit, core[4], expected)
    };

    let code = institution_code_from_str(code_text).ok_or(GuardError::InvalidCid)?;
    if is_three_char_code(&code) != (code_text.len() == 3) || checksum != expected_checksum {
        return Err(GuardError::InvalidCid);
    }
    match profit_policy(&code) {
        Some(ProfitPolicy::NonProfit) if profit => return Err(GuardError::InvalidCid),
        Some(ProfitPolicy::Profit) if !profit => return Err(GuardError::InvalidCid),
        Some(_) => {}
        None => return Err(GuardError::InvalidCid),
    }
    if !valid_province_city(code, r5) {
        return Err(GuardError::InvalidCid);
    }
    Ok(code)
}

fn validate_cid_namespace(cid: &[u8], namespace: Namespace) -> Result<InstitutionCode, GuardError> {
    let code = parse_cid(cid)?;
    let valid = match namespace {
        Namespace::Public => is_public_legal_code(&code) || is_fixed_governance_code(&code),
        Namespace::Private => is_private_legal_code(&code) || is_unincorporated_code(&code),
    };
    if !valid {
        return Err(GuardError::InvalidCidNamespace);
    }
    if is_fixed_governance_code(&code)
        && primitives::governance_skeleton::fixed_institution_by_cid(cid)
            .is_none_or(|institution| institution.code != code)
    {
        return Err(GuardError::InstitutionIdentityChanged);
    }
    if primitives::institution_constraints::is_permanent_singleton_code(&code)
        && primitives::institution_constraints::singleton_by_cid(cid)
            .is_none_or(|institution| institution.code != code)
    {
        return Err(GuardError::SingletonInstitutionIdentityMismatch);
    }
    Ok(code)
}

fn validate_citizen_record(
    cid: &[u8],
    record: &CitizenCidRecord,
    block: Option<u32>,
) -> Result<(), GuardError> {
    // 人主体 CID(公民 CTZN / 居民 NATP / 智能人 SMTP)统一放行:占号入口接受 CTZN|NATP
    // (自助/注册局占即绑),与 `valid_province_city` 的 is_person_code 去地域化判定一致。
    if !is_person_code(&parse_cid(cid)?) || record.registrar_cid_number.is_empty() {
        return Err(GuardError::InvalidCidNamespace);
    }
    match record.status {
        CitizenCidStatus::Active if record.revoked_at.is_none() => {}
        CitizenCidStatus::Revoked if record.revoked_at.is_some() => {}
        _ => return Err(GuardError::CitizenCidStatusInvalid),
    }
    if let Some(block) = block {
        if record.registered_at > block || record.revoked_at.is_some_and(|at| at > block) {
            return Err(GuardError::CitizenCidRevocationHeightInvalid);
        }
    }
    Ok(())
}

fn validate_institution_record(
    namespace: Namespace,
    cid: &[u8],
    record: &InstitutionRecord,
) -> Result<(), GuardError> {
    if validate_cid_namespace(cid, namespace)? != record.institution_code {
        return Err(GuardError::InstitutionIdentityChanged);
    }
    if let Some(representative) = &record.legal_representative {
        if representative.family_name.is_empty()
            || representative.given_name.is_empty()
            || representative.cid_number.is_empty()
        {
            return Err(GuardError::InstitutionLegalRepresentativeInvalid);
        }
    }
    Ok(())
}

fn validate_account_record(
    cid: &[u8],
    name: &[u8],
    record: &InstitutionAccountRecord,
) -> Result<(), GuardError> {
    let kind = institution_kind_by_name(cid, name).ok_or(GuardError::AccountAddressMismatch)?;
    if kind.derive(SS58_FORMAT) != record.address {
        return Err(GuardError::AccountAddressMismatch);
    }
    Ok(())
}

fn validate_account_reverse<F>(
    namespace: Namespace,
    cid: &[u8],
    name: &[u8],
    record: &InstitutionAccountRecord,
    read: &F,
) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let reverse_raw = read(&storage_key::account_registered(namespace, &record.address))
        .ok_or(GuardError::AccountReverseIndexMissing)?;
    let reverse: RegisteredInstitution = decode_exact(&reverse_raw, "AccountRegisteredCid")?;
    if reverse.cid_number != cid || reverse.account_name != name {
        return Err(GuardError::AccountReverseIndexMismatch);
    }
    Ok(())
}

fn validate_required_accounts<F>(
    namespace: Namespace,
    cid: &[u8],
    record: &InstitutionRecord,
    read: &F,
) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    // 这里校验「必备账户是否都在」：主/费用等恒必备项与父级无关，故传 None。
    // UNIN 的清算账户是否必备取决于父级（链上不保存），不算必备项；它的合法性
    // 由「允许集」`allowed_protocol_account_kinds` 兜住。
    let required = primitives::institution_constraints::required_protocol_account_kinds(
        record.institution_code,
        cid,
        None,
    )
    .ok_or(GuardError::InstitutionIdentityChanged)?;
    for kind in required {
        let name = institution_protocol_account_name(*kind);
        let raw = read(&storage_key::institution_account_id(namespace, cid, name))
            .ok_or(GuardError::RequiredProtocolAccountMissing)?;
        let account: InstitutionAccountRecord = decode_exact(&raw, "InstitutionAccounts")?;
        validate_account_record(cid, name, &account)?;
        validate_account_reverse(namespace, cid, name, &account, read)?;
    }
    Ok(())
}

fn check_citizen_transition(
    block: u32,
    cid: &[u8],
    parent_raw: Option<Vec<u8>>,
    post_raw: Option<Vec<u8>>,
) -> Result<(), GuardError> {
    let post_raw = post_raw.ok_or(GuardError::CitizenCidDeleted)?;
    let post: CitizenCidRecord = decode_exact(&post_raw, "CidRegistry")?;
    validate_citizen_record(cid, &post, Some(block))?;
    let Some(parent_raw) = parent_raw else {
        if post.registered_at != block
            || (post.status == CitizenCidStatus::Revoked && post.revoked_at != Some(block))
        {
            return Err(GuardError::CitizenCidRevocationHeightInvalid);
        }
        return Ok(());
    };
    let parent: CitizenCidRecord = decode_exact(&parent_raw, "CidRegistry")?;
    validate_citizen_record(cid, &parent, None)?;
    if parent.registrar_cid_number != post.registrar_cid_number
        || parent.commitment != post.commitment
        || parent.residence_province_code != post.residence_province_code
        || parent.residence_city_code != post.residence_city_code
        || parent.registered_at != post.registered_at
    {
        return Err(GuardError::CitizenCidIdentityChanged);
    }
    match (parent.status, post.status) {
        (CitizenCidStatus::Active, CitizenCidStatus::Active) if parent_raw == post_raw => Ok(()),
        (CitizenCidStatus::Active, CitizenCidStatus::Revoked) if post.revoked_at == Some(block) => {
            Ok(())
        }
        (CitizenCidStatus::Revoked, CitizenCidStatus::Revoked) if parent_raw == post_raw => Ok(()),
        _ => Err(GuardError::CitizenCidStatusInvalid),
    }
}

fn validate_candidate_identity<F>(cid: &[u8], read: &F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let Some(candidate_raw) = read(&storage_key::candidate_identity(cid)) else {
        return Ok(());
    };
    let _: CitizenCandidateIdentity = decode_exact(&candidate_raw, "CandidateIdentityByCid")?;
    let registry_raw =
        read(&storage_key::citizen_registry(cid)).ok_or(GuardError::CitizenIdentityWithoutCid)?;
    let registry: CitizenCidRecord = decode_exact(&registry_raw, "CidRegistry")?;
    if registry.status != CitizenCidStatus::Active {
        return Err(GuardError::CitizenCandidateIdentityInvalidStatus);
    }
    let voting_raw = read(&storage_key::voting_identity(cid))
        .ok_or(GuardError::CitizenCandidateIdentityWithoutVotingIdentity)?;
    let voting: CitizenVotingIdentity = decode_exact(&voting_raw, "VotingIdentityByCid")?;
    if voting.citizen_status != CitizenStatus::Normal {
        return Err(GuardError::CitizenCandidateIdentityInvalidStatus);
    }
    Ok(())
}

fn check_candidate_transition<FPost>(
    block: u32,
    cid: &[u8],
    parent_raw: Option<Vec<u8>>,
    post_raw: Option<Vec<u8>>,
    post: &FPost,
) -> Result<(), GuardError>
where
    FPost: Fn(&[u8]) -> Option<Vec<u8>>,
{
    match (parent_raw, post_raw) {
        (None, Some(after_raw)) => {
            let after: CitizenCandidateIdentity =
                decode_exact(&after_raw, "CandidateIdentityByCid")?;
            if after.updated_at != block {
                return Err(GuardError::CitizenCandidateIdentityUpdateHeightInvalid);
            }
            validate_candidate_identity(cid, post)
        }
        (Some(before_raw), Some(after_raw)) => {
            let before: CitizenCandidateIdentity =
                decode_exact(&before_raw, "CandidateIdentityByCid")?;
            let after: CitizenCandidateIdentity =
                decode_exact(&after_raw, "CandidateIdentityByCid")?;
            if before.birth_province_code != after.birth_province_code
                || before.birth_city_code != after.birth_city_code
                || before.birth_town_code != after.birth_town_code
                || before.citizen_sex != after.citizen_sex
                || before.birth_date != after.birth_date
            {
                return Err(GuardError::CitizenCandidateIdentityLockedFieldChanged);
            }
            if before_raw != after_raw && after.updated_at != block {
                return Err(GuardError::CitizenCandidateIdentityUpdateHeightInvalid);
            }
            validate_candidate_identity(cid, post)
        }
        (Some(_), None) => {
            let registry_raw = post(&storage_key::citizen_registry(cid))
                .ok_or(GuardError::CitizenCandidateIdentityDeleted)?;
            let registry: CitizenCidRecord = decode_exact(&registry_raw, "CidRegistry")?;
            let voting_raw = post(&storage_key::voting_identity(cid))
                .ok_or(GuardError::CitizenCandidateIdentityDeleted)?;
            let voting: CitizenVotingIdentity = decode_exact(&voting_raw, "VotingIdentityByCid")?;
            if registry.status != CitizenCidStatus::Revoked
                || voting.citizen_status != CitizenStatus::Revoked
            {
                return Err(GuardError::CitizenCandidateIdentityDeleted);
            }
            Ok(())
        }
        (None, None) => Ok(()),
    }
}

fn validate_citizen_identity_binding<F>(cid: &[u8], read: &F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let registry_raw =
        read(&storage_key::citizen_registry(cid)).ok_or(GuardError::CitizenIdentityWithoutCid)?;
    let registry: CitizenCidRecord = decode_exact(&registry_raw, "CidRegistry")?;
    validate_citizen_record(cid, &registry, None)?;

    // 投票身份、竞选身份是用户可选项,不是必须:匿名 CID(占号即绑账户、无投票身份)
    // 合法。存在则校验可解码,不存在则放行——守卫不强制任何身份信息,用户只凭钱包账户
    // 即可占号绑定。投票身份一旦创建不得删除的单调性由 `check_transition` 单独兜底。
    if let Some(identity_raw) = read(&storage_key::voting_identity(cid)) {
        let _: CitizenVotingIdentity = decode_exact(&identity_raw, "VotingIdentityByCid")?;
    }
    validate_candidate_identity(cid, read)?;

    let account_raw = read(&storage_key::account_id_by_cid(cid))
        .ok_or(GuardError::CitizenAccountIdBindingMissing)?;
    let account: [u8; 32] = decode_exact(&account_raw, "AccountIdByCid")?;
    let reverse_raw = read(&storage_key::cid_by_account_id(&account))
        .ok_or(GuardError::CitizenAccountIdBindingMissing)?;
    let reverse_cid: Vec<u8> = decode_exact(&reverse_raw, "CidByAccountId")?;
    if reverse_cid != cid {
        return Err(GuardError::CitizenAccountIdBindingMismatch);
    }
    let revision_raw = read(&storage_key::binding_revision_by_cid(cid))
        .ok_or(GuardError::CitizenBindingRevisionMissing)?;
    let revision: u64 = decode_exact(&revision_raw, "BindingRevisionByCid")?;
    if revision == 0 {
        return Err(GuardError::CitizenBindingRevisionInvalid);
    }
    Ok(())
}

/// 普通区块只对本块触及的 CID、机构和账户执行单调性与正反索引校验。
pub fn check_transition<FParent, FPost>(
    block: u32,
    delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    parent: FParent,
    post: FPost,
    reference: &GenesisReference,
) -> Result<(), GuardError>
where
    FParent: Fn(&[u8]) -> Option<Vec<u8>>,
    FPost: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let citizen_prefix = storage_key::citizen_registry_prefix();
    let mut required_revision_changes = BTreeSet::<Vec<u8>>::new();
    let mut touched_citizens = BTreeSet::<Vec<u8>>::new();
    for key in delta.keys().filter(|key| key.starts_with(&citizen_prefix)) {
        let cid = parse_vec_map_key(key, &citizen_prefix, "CidRegistry")?;
        let before = parent(key);
        let after = post(key);
        check_citizen_transition(block, &cid, before.clone(), after.clone())?;
        touched_citizens.insert(cid.clone());
        if let (Some(before), Some(after)) = (before, after) {
            let before: CitizenCidRecord = decode_exact(&before, "CidRegistry")?;
            let after: CitizenCidRecord = decode_exact(&after, "CidRegistry")?;
            if before.status == CitizenCidStatus::Active
                && after.status == CitizenCidStatus::Revoked
            {
                required_revision_changes.insert(cid);
            }
        }
    }

    let identity_prefix = storage_key::voting_identity_prefix();
    let candidate_prefix = storage_key::candidate_identity_prefix();
    let forward_prefix = storage_key::account_id_by_cid_prefix();
    let reverse_prefix = storage_key::cid_by_account_id_prefix();
    let revision_prefix = storage_key::binding_revision_by_cid_prefix();
    let mut forward_touched = BTreeSet::<Vec<u8>>::new();
    for key in delta.keys() {
        if key.starts_with(&identity_prefix) {
            let cid = parse_vec_map_key(key, &identity_prefix, "VotingIdentityByCid")?;
            if post(key).is_none() {
                return Err(GuardError::CitizenIdentityDeleted);
            }
            touched_citizens.insert(cid);
        }
        if key.starts_with(&candidate_prefix) {
            let cid = parse_vec_map_key(key, &candidate_prefix, "CandidateIdentityByCid")?;
            check_candidate_transition(block, &cid, parent(key), post(key), &post)?;
            touched_citizens.insert(cid);
        }
        if key.starts_with(&forward_prefix) {
            let cid = parse_vec_map_key(key, &forward_prefix, "AccountIdByCid")?;
            // 净值变化必须同步推进 revision；仅触及但净值相同还可能是合法的
            // self_occupy 幂等重试，也可能是同块 A→B→A，需结合 revision delta 判定。
            if parent(key) != post(key) {
                required_revision_changes.insert(cid.clone());
            }
            forward_touched.insert(cid.clone());
            touched_citizens.insert(cid);
        }
        if key.starts_with(&reverse_prefix) {
            let account = parse_account_map_key(key, &reverse_prefix, "CidByAccountId")?;
            if let Some(raw) = post(key) {
                let cid: Vec<u8> = decode_exact(&raw, "CidByAccountId")?;
                let forward_raw = post(&storage_key::account_id_by_cid(&cid))
                    .ok_or(GuardError::CitizenAccountIdBindingMissing)?;
                let forward: [u8; 32] = decode_exact(&forward_raw, "AccountIdByCid")?;
                if forward != account {
                    return Err(GuardError::CitizenAccountIdBindingMismatch);
                }
                touched_citizens.insert(cid);
            } else if let Some(raw) = parent(key) {
                let cid: Vec<u8> = decode_exact(&raw, "CidByAccountId")?;
                if post(&storage_key::account_id_by_cid(&cid)).is_some_and(|raw| {
                    decode_exact::<[u8; 32]>(&raw, "AccountIdByCid") == Ok(account)
                }) {
                    return Err(GuardError::CitizenAccountIdBindingMismatch);
                }
                touched_citizens.insert(cid);
            }
        }
    }
    for key in delta.keys().filter(|key| key.starts_with(&revision_prefix)) {
        let cid = parse_vec_map_key(key, &revision_prefix, "BindingRevisionByCid")?;
        let after_raw = post(key).ok_or(GuardError::CitizenBindingRevisionMissing)?;
        let after: u64 = decode_exact(&after_raw, "BindingRevisionByCid")?;
        let valid = match parent(key) {
            None => {
                after == 1
                    && parent(&storage_key::account_id_by_cid(&cid)).is_none()
                    && post(&storage_key::account_id_by_cid(&cid)).is_some()
            }
            Some(before_raw) => {
                let before: u64 = decode_exact(&before_raw, "BindingRevisionByCid")?;
                // NodeGuard 只观察块前/块后：同块可合法发生 A→B→C 或换绑后吊销，
                // 也可发生 A→B→A；因此要求严格单调并有 forward 触及或吊销证据。
                // 每次调用恰好 +1 由 runtime 单测锁定。
                after > before
                    && (forward_touched.contains(&cid) || required_revision_changes.contains(&cid))
            }
        };
        if !valid {
            return Err(GuardError::CitizenBindingRevisionTransitionInvalid);
        }
        required_revision_changes.remove(&cid);
        touched_citizens.insert(cid);
    }
    if !required_revision_changes.is_empty() {
        return Err(GuardError::CitizenBindingRevisionTransitionInvalid);
    }
    for cid in touched_citizens {
        validate_citizen_identity_binding(&cid, &post)?;
    }

    let mut touched_institutions = BTreeSet::<(u8, Vec<u8>)>::new();
    for namespace in [Namespace::Public, Namespace::Private] {
        let namespace_id = if namespace == Namespace::Public { 0 } else { 1 };
        let institution_prefix = storage_key::institution_prefix(namespace);
        let account_prefix = storage_key::institution_account_prefix(namespace);
        let reverse_prefix = storage_key::account_registered_prefix(namespace);

        for key in delta.keys() {
            if key.starts_with(&institution_prefix) {
                let cid = parse_vec_map_key(key, &institution_prefix, "Institutions")?;
                let protected = reference.institutions.get(&(namespace_id, cid.clone()));
                let Some(post_raw) = post(key) else {
                    if protected.is_some() {
                        return Err(GuardError::GenesisInstitutionDeleted);
                    }
                    // 普通机构可以由 runtime 依法删除；关联账户和索引由后续全状态检查
                    // 确认已经同时清空，NodeGuard 不再把普通主体永久化。
                    continue;
                };
                let post_record: InstitutionRecord = decode_exact(&post_raw, "Institutions")?;
                validate_institution_record(namespace, &cid, &post_record)?;
                if post(&storage_key::institution(namespace.sibling(), &cid)).is_some() {
                    return Err(GuardError::CrossNamespaceDuplicate);
                }
                if parent(key).is_some() {
                    if protected.is_some_and(|identity| {
                        identity.institution_code != post_record.institution_code
                            || identity.created_at != post_record.created_at
                            || identity.town_code != post_record.town_code
                    }) {
                        return Err(GuardError::GenesisInstitutionChanged);
                    }
                } else if is_fixed_governance_code(&post_record.institution_code)
                    || primitives::institution_constraints::is_permanent_singleton_code(
                        &post_record.institution_code,
                    )
                {
                    return Err(GuardError::FixedInstitutionCreatedAfterGenesis);
                }
                touched_institutions.insert((namespace_id, cid));
            }

            if key.starts_with(&account_prefix) {
                let (cid, name) =
                    parse_double_vec_key(key, &account_prefix, "InstitutionAccounts")?;
                let institution = post(&storage_key::institution(namespace, &cid))
                    .map(|raw| decode_exact::<InstitutionRecord>(&raw, "Institutions"))
                    .transpose()?;
                if let Some(institution) = &institution {
                    validate_institution_record(namespace, &cid, institution)?;
                }
                match (parent(key), post(key)) {
                    (Some(before), Some(after)) if before == after => {
                        if institution.is_none() {
                            return Err(GuardError::AccountWithoutInstitution);
                        }
                    }
                    (Some(_), Some(_)) => return Err(GuardError::AccountChanged),
                    (Some(before), None) => {
                        let account: InstitutionAccountRecord =
                            decode_exact(&before, "InstitutionAccounts")?;
                        if institution.is_some() {
                            let kind = institution_kind_by_name(&cid, &name)
                                .ok_or(GuardError::AccountAddressMismatch)?;
                            if !kind.is_closable_institution_account() {
                                return Err(GuardError::ProtocolAccountDeleted);
                            }
                        }
                        if post(&storage_key::account_registered(
                            namespace,
                            &account.address,
                        ))
                        .is_some()
                        {
                            return Err(GuardError::AccountReverseIndexMismatch);
                        }
                    }
                    (None, Some(after)) => {
                        let institution = institution
                            .as_ref()
                            .ok_or(GuardError::AccountWithoutInstitution)?;
                        let account: InstitutionAccountRecord =
                            decode_exact(&after, "InstitutionAccounts")?;
                        validate_account_record(&cid, &name, &account)?;
                        if let Some(protocol_kind) = institution_protocol_kind_by_name(&name) {
                            // 用「允许集」而非「必备集」：链上不保存 UNIN 的父级，守卫
                            // 无从判定它是不是股份公司分支，故其清算账户必须可有可无。
                            let allowed =
                                primitives::institution_constraints::allowed_protocol_account_kinds(
                                    institution.institution_code,
                                    &cid,
                                )
                                .ok_or(GuardError::InstitutionIdentityChanged)?;
                            if !allowed.contains(&protocol_kind) {
                                return Err(GuardError::UnexpectedProtocolAccount);
                            }
                        }
                        validate_account_reverse(namespace, &cid, &name, &account, &post)?;
                    }
                    (None, None) => {}
                }
                if institution.is_some() {
                    touched_institutions.insert((namespace_id, cid));
                }
            }

            if key.starts_with(&reverse_prefix) {
                let account = parse_account_map_key(key, &reverse_prefix, "AccountRegisteredCid")?;
                if let Some(raw) = post(key) {
                    let registered: RegisteredInstitution =
                        decode_exact(&raw, "AccountRegisteredCid")?;
                    let forward_raw = post(&storage_key::institution_account_id(
                        namespace,
                        &registered.cid_number,
                        &registered.account_name,
                    ))
                    .ok_or(GuardError::AccountReverseIndexMismatch)?;
                    let forward: InstitutionAccountRecord =
                        decode_exact(&forward_raw, "InstitutionAccounts")?;
                    if forward.address != account {
                        return Err(GuardError::AccountReverseIndexMismatch);
                    }
                    touched_institutions.insert((namespace_id, registered.cid_number));
                }
            }
        }
    }

    for (namespace_id, cid) in touched_institutions {
        let namespace = if namespace_id == 0 {
            Namespace::Public
        } else {
            Namespace::Private
        };
        let raw = post(&storage_key::institution(namespace, &cid))
            .ok_or(GuardError::AccountWithoutInstitution)?;
        let record: InstitutionRecord = decode_exact(&raw, "Institutions")?;
        validate_required_accounts(namespace, &cid, &record, &post)?;
    }
    Ok(())
}

fn validate_full_state<F>(
    keys: &[Vec<u8>],
    read: &F,
    reference: Option<&GenesisReference>,
) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let citizen_prefix = storage_key::citizen_registry_prefix();
    for key in keys.iter().filter(|key| key.starts_with(&citizen_prefix)) {
        let Some(raw) = read(key) else { continue };
        let cid = parse_vec_map_key(key, &citizen_prefix, "CidRegistry")?;
        let record: CitizenCidRecord = decode_exact(&raw, "CidRegistry")?;
        validate_citizen_record(&cid, &record, None)?;
        // 每个 Active/Revoked CID 都必须存在唯一账户正反绑定和非零 revision；
        // 仅有登记表记录的半状态同样拒绝。
        validate_citizen_identity_binding(&cid, read)?;
    }

    let identity_prefix = storage_key::voting_identity_prefix();
    for key in keys.iter().filter(|key| key.starts_with(&identity_prefix)) {
        let Some(_) = read(key) else { continue };
        let cid = parse_vec_map_key(key, &identity_prefix, "VotingIdentityByCid")?;
        validate_citizen_identity_binding(&cid, read)?;
    }

    let candidate_prefix = storage_key::candidate_identity_prefix();
    for key in keys.iter().filter(|key| key.starts_with(&candidate_prefix)) {
        let Some(_) = read(key) else { continue };
        let cid = parse_vec_map_key(key, &candidate_prefix, "CandidateIdentityByCid")?;
        validate_citizen_identity_binding(&cid, read)?;
    }

    let forward_prefix = storage_key::account_id_by_cid_prefix();
    for key in keys.iter().filter(|key| key.starts_with(&forward_prefix)) {
        let Some(_) = read(key) else { continue };
        let cid = parse_vec_map_key(key, &forward_prefix, "AccountIdByCid")?;
        validate_citizen_identity_binding(&cid, read)?;
    }

    let reverse_prefix = storage_key::cid_by_account_id_prefix();
    for key in keys.iter().filter(|key| key.starts_with(&reverse_prefix)) {
        let Some(raw) = read(key) else { continue };
        let account = parse_account_map_key(key, &reverse_prefix, "CidByAccountId")?;
        let cid: Vec<u8> = decode_exact(&raw, "CidByAccountId")?;
        let forward_raw = read(&storage_key::account_id_by_cid(&cid))
            .ok_or(GuardError::CitizenAccountIdBindingMissing)?;
        let forward: [u8; 32] = decode_exact(&forward_raw, "AccountIdByCid")?;
        if forward != account {
            return Err(GuardError::CitizenAccountIdBindingMismatch);
        }
        validate_citizen_identity_binding(&cid, read)?;
    }

    let revision_prefix = storage_key::binding_revision_by_cid_prefix();
    for key in keys.iter().filter(|key| key.starts_with(&revision_prefix)) {
        let Some(raw) = read(key) else { continue };
        let cid = parse_vec_map_key(key, &revision_prefix, "BindingRevisionByCid")?;
        let revision: u64 = decode_exact(&raw, "BindingRevisionByCid")?;
        if revision == 0 {
            return Err(GuardError::CitizenBindingRevisionInvalid);
        }
        validate_citizen_identity_binding(&cid, read)?;
    }

    let mut occupied_public = BTreeSet::new();
    let mut occupied_private = BTreeSet::new();
    for namespace in [Namespace::Public, Namespace::Private] {
        let institution_prefix = storage_key::institution_prefix(namespace);
        let account_prefix = storage_key::institution_account_prefix(namespace);
        let reverse_prefix = storage_key::account_registered_prefix(namespace);
        let occupied = if namespace == Namespace::Public {
            &mut occupied_public
        } else {
            &mut occupied_private
        };

        for key in keys
            .iter()
            .filter(|key| key.starts_with(&institution_prefix))
        {
            let Some(raw) = read(key) else { continue };
            let cid = parse_vec_map_key(key, &institution_prefix, "Institutions")?;
            let record: InstitutionRecord = decode_exact(&raw, "Institutions")?;
            validate_institution_record(namespace, &cid, &record)?;
            validate_required_accounts(namespace, &cid, &record, read)?;
            occupied.insert(cid);
        }

        for key in keys.iter().filter(|key| key.starts_with(&account_prefix)) {
            let Some(raw) = read(key) else { continue };
            let (cid, name) = parse_double_vec_key(key, &account_prefix, "InstitutionAccounts")?;
            if !occupied.contains(&cid) {
                return Err(GuardError::AccountWithoutInstitution);
            }
            let record: InstitutionAccountRecord = decode_exact(&raw, "InstitutionAccounts")?;
            validate_account_record(&cid, &name, &record)?;
            validate_account_reverse(namespace, &cid, &name, &record, read)?;
            if let Some(protocol_kind) = institution_protocol_kind_by_name(&name) {
                let institution_raw = read(&storage_key::institution(namespace, &cid))
                    .ok_or(GuardError::AccountWithoutInstitution)?;
                let institution: InstitutionRecord =
                    decode_exact(&institution_raw, "Institutions")?;
                // 同上：用「允许集」，否则会把带清算账户的合法银行分支行判成非法。
                let allowed = primitives::institution_constraints::allowed_protocol_account_kinds(
                    institution.institution_code,
                    &cid,
                )
                .ok_or(GuardError::InstitutionIdentityChanged)?;
                if !allowed.contains(&protocol_kind) {
                    return Err(GuardError::UnexpectedProtocolAccount);
                }
            }
        }

        for key in keys.iter().filter(|key| key.starts_with(&reverse_prefix)) {
            let Some(raw) = read(key) else { continue };
            let account = parse_account_map_key(key, &reverse_prefix, "AccountRegisteredCid")?;
            let registered: RegisteredInstitution = decode_exact(&raw, "AccountRegisteredCid")?;
            let forward_raw = read(&storage_key::institution_account_id(
                namespace,
                &registered.cid_number,
                &registered.account_name,
            ))
            .ok_or(GuardError::AccountReverseIndexMismatch)?;
            let forward: InstitutionAccountRecord =
                decode_exact(&forward_raw, "InstitutionAccounts")?;
            if forward.address != account {
                return Err(GuardError::AccountReverseIndexMismatch);
            }
        }
    }

    if occupied_public
        .iter()
        .any(|cid| occupied_private.contains(cid))
    {
        return Err(GuardError::CrossNamespaceDuplicate);
    }

    if let Some(reference) = reference {
        for ((namespace_id, cid), identity) in &reference.institutions {
            let namespace = if *namespace_id == Namespace::Public.id() {
                Namespace::Public
            } else {
                Namespace::Private
            };
            let raw = read(&storage_key::institution(namespace, cid))
                .ok_or(GuardError::GenesisInstitutionDeleted)?;
            let record: InstitutionRecord = decode_exact(&raw, "Institutions")?;
            if record.institution_code != identity.institution_code
                || record.created_at != identity.created_at
                || record.town_code != identity.town_code
            {
                return Err(GuardError::GenesisInstitutionChanged);
            }
            if read(&storage_key::institution(namespace.sibling(), cid)).is_some() {
                return Err(GuardError::CrossNamespaceDuplicate);
            }
        }
    }

    for singleton in primitives::institution_constraints::singleton_institutions() {
        let cid = singleton.cid_number.as_bytes();
        let raw = read(&storage_key::institution(Namespace::Public, cid))
            .ok_or(GuardError::SingletonInstitutionMissing)?;
        let record: InstitutionRecord = decode_exact(&raw, "Institutions")?;
        if record.institution_code != singleton.code {
            return Err(GuardError::SingletonInstitutionIdentityMismatch);
        }
        let main_raw = read(&storage_key::institution_account_id(
            Namespace::Public,
            cid,
            institution_protocol_account_name(InstitutionProtocolAccountKind::Main),
        ))
        .ok_or(GuardError::SingletonInstitutionMissing)?;
        let main: InstitutionAccountRecord = decode_exact(&main_raw, "InstitutionAccounts")?;
        if main.address != singleton.main_account {
            return Err(GuardError::SingletonInstitutionIdentityMismatch);
        }
    }
    Ok(())
}

impl GenesisReference {
    pub fn from_genesis<F>(keys: &[Vec<u8>], read: F) -> Result<Self, GuardError>
    where
        F: Fn(&[u8]) -> Option<Vec<u8>>,
    {
        validate_full_state(keys, &read, None)?;
        let mut institutions = BTreeMap::new();
        for namespace in [Namespace::Public, Namespace::Private] {
            let prefix = storage_key::institution_prefix(namespace);
            for key in keys.iter().filter(|key| key.starts_with(&prefix)) {
                let Some(raw) = read(key) else { continue };
                let cid = parse_vec_map_key(key, &prefix, "Institutions")?;
                let record: InstitutionRecord = decode_exact(&raw, "Institutions")?;
                institutions.insert(
                    (namespace.id(), cid),
                    GenesisInstitutionIdentity {
                        institution_code: record.institution_code,
                        town_code: record.town_code,
                        created_at: record.created_at,
                    },
                );
            }
        }
        Ok(Self { institutions })
    }
}

pub fn check_full_state<F>(
    keys: &[Vec<u8>],
    read: F,
    reference: &GenesisReference,
) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    validate_full_state(keys, &read, Some(reference))
}

pub fn check_imported_genesis<'a, I>(
    pairs: I,
    reference: &GenesisReference,
) -> Result<(), GuardError>
where
    I: IntoIterator<Item = (&'a Vec<u8>, &'a Vec<u8>)>,
{
    let prefixes = storage_key::relevant_prefixes();
    let map: BTreeMap<Vec<u8>, Vec<u8>> = pairs
        .into_iter()
        .filter(|(key, _)| matches_relevant_prefixes(key, &prefixes))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let keys: Vec<Vec<u8>> = map.keys().cloned().collect();
    check_full_state(&keys, |key| map.get(key).cloned(), reference)
}

/// 当前规则只保护 block#0 精确机构集合与导入态完整性，不再把普通机构生命周期
/// 误当作永久单调历史，因此任意高度的完整状态都可以独立验证。
pub fn check_state_import_height(_block: u32) -> Result<(), GuardError> {
    Ok(())
}

pub fn needs_full_check(delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>) -> bool {
    if delta.contains_key(CODE_KEY) {
        return true;
    }
    [Namespace::Public, Namespace::Private]
        .into_iter()
        .map(storage_key::institution_prefix)
        .any(|prefix| {
            delta
                .iter()
                .any(|(key, value)| key.starts_with(&prefix) && value.is_none())
        })
}

pub fn is_relevant_key(key: &[u8]) -> bool {
    matches_relevant_prefixes(key, &storage_key::relevant_prefixes())
}

pub fn matches_relevant_prefixes(key: &[u8], prefixes: &[Vec<u8>]) -> bool {
    prefixes.iter().any(|prefix| key.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cid_with_unchecked_tail(
        r5: &[u8],
        code: &[u8],
        profit: u8,
        n9: &[u8],
        year: &[u8],
    ) -> Vec<u8> {
        let total = checksum_acc(&[r5, code, &[profit], n9, year]);
        let checksum = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"[total % 36];
        [r5, b"-", code, &[profit, checksum], b"-", n9, b"-", year].concat()
    }

    fn private_record() -> InstitutionRecord {
        InstitutionRecord {
            cid_full_name: b"ordinary company".to_vec(),
            cid_short_name: b"company".to_vec(),
            town_code: Vec::new(),
            legal_representative: None,
            institution_code: *b"SFGQ",
            created_at: 10,
        }
    }

    fn citizen_cid_number(tag: &str) -> Vec<u8> {
        primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: tag,
                p1: "1",
                province_code: "GD",
                province_name: "广东省",
                city_code: "001",
                city_name: "荔湾市",
                year: "2026",
                institution: "CTZN",
            },
        )
        .expect("valid citizen CID")
        .into_bytes()
    }

    fn natp_cid_number(tag: &str) -> Vec<u8> {
        primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: tag,
                p1: "1",
                province_code: "GD",
                province_name: "广东省",
                city_code: "001",
                city_name: "荔湾市",
                year: "2026",
                institution: "NATP",
            },
        )
        .expect("valid resident CID")
        .into_bytes()
    }

    fn citizen_state(cid: &[u8], account: [u8; 32]) -> BTreeMap<Vec<u8>, Vec<u8>> {
        BTreeMap::from([
            (
                storage_key::citizen_registry(cid),
                CitizenCidRecord {
                    registrar_cid_number: b"registrar".to_vec(),
                    commitment: [3u8; 32],
                    residence_province_code: b"GD".to_vec(),
                    residence_city_code: b"001".to_vec(),
                    status: CitizenCidStatus::Active,
                    registered_at: 1,
                    revoked_at: None,
                }
                .encode(),
            ),
            (
                storage_key::voting_identity(cid),
                (
                    20260101u32,
                    20360101u32,
                    CitizenStatus::Normal,
                    b"GD".to_vec(),
                    b"001".to_vec(),
                    Vec::<u8>::new(),
                    1u32,
                )
                    .encode(),
            ),
            (storage_key::account_id_by_cid(cid), account.encode()),
            (
                storage_key::cid_by_account_id(&account),
                cid.to_vec().encode(),
            ),
            (storage_key::binding_revision_by_cid(cid), 1u64.encode()),
        ])
    }

    /// 匿名 CID 状态:占号即绑账户(登记记录空居住地 + 账户双向绑定),**无投票身份**。
    /// 用于验证守卫接受「用户只提供钱包账户」的匿名注册(身份是可选项、非必须)。
    fn anonymous_citizen_state(cid: &[u8], account: [u8; 32]) -> BTreeMap<Vec<u8>, Vec<u8>> {
        BTreeMap::from([
            (
                storage_key::citizen_registry(cid),
                CitizenCidRecord {
                    registrar_cid_number: b"registrar".to_vec(),
                    commitment: [3u8; 32],
                    residence_province_code: Vec::new(),
                    residence_city_code: Vec::new(),
                    status: CitizenCidStatus::Active,
                    registered_at: 1,
                    revoked_at: None,
                }
                .encode(),
            ),
            (storage_key::account_id_by_cid(cid), account.encode()),
            (
                storage_key::cid_by_account_id(&account),
                cid.to_vec().encode(),
            ),
            (storage_key::binding_revision_by_cid(cid), 1u64.encode()),
        ])
    }

    fn candidate_identity(updated_at: u32) -> CitizenCandidateIdentity {
        CitizenCandidateIdentity {
            birth_province_code: b"GD".to_vec(),
            birth_city_code: b"001".to_vec(),
            birth_town_code: b"001001".to_vec(),
            family_name: b"Guard".to_vec(),
            given_name: b"Candidate".to_vec(),
            citizen_sex: CitizenSex::Female,
            birth_date: 20000101,
            updated_at,
        }
    }

    #[test]
    fn cid_guard_checks_only_province_city_code_profit_and_checksum() {
        // 随机段和年份段故意使用非数字、非标准长度；只要四项受保护规则及由原始
        // 载荷计算出的校验位正确，NodeGuard 就不额外背书 N9/年份派生规则。
        let cid = cid_with_unchecked_tail(b"GD001", b"NRC", b'0', b"X", b"");
        assert_eq!(parse_cid(&cid), Ok(*b"NRC\0"));

        let bad_city = cid_with_unchecked_tail(b"GD999", b"NRC", b'0', b"X", b"");
        assert_eq!(parse_cid(&bad_city), Err(GuardError::InvalidCid));

        let mut bad_checksum = cid;
        bad_checksum[10] = if bad_checksum[10] == b'Z' { b'Y' } else { b'Z' };
        assert_eq!(parse_cid(&bad_checksum), Err(GuardError::InvalidCid));
    }

    #[test]
    fn ordinary_institution_can_be_deleted_but_genesis_institution_cannot() {
        let cid = primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: "ordinary",
                p1: "1",
                province_code: "GD",
                province_name: "广东省",
                city_code: "001",
                city_name: "荔湾市",
                year: "2026",
                institution: "SFGQ",
            },
        )
        .expect("valid private CID")
        .into_bytes();
        let key = storage_key::institution(Namespace::Private, &cid);
        let before = private_record().encode();
        let delta = BTreeMap::from([(key.clone(), None)]);
        let parent = BTreeMap::from([(key.clone(), before)]);
        let post = BTreeMap::<Vec<u8>, Vec<u8>>::new();

        assert_eq!(
            check_transition(
                11,
                &delta,
                |raw| parent.get(raw).cloned(),
                |raw| post.get(raw).cloned(),
                &GenesisReference::default(),
            ),
            Ok(())
        );

        let reference = GenesisReference {
            institutions: BTreeMap::from([(
                (Namespace::Private.id(), cid),
                GenesisInstitutionIdentity {
                    institution_code: *b"SFGQ",
                    town_code: Vec::new(),
                    created_at: 10,
                },
            )]),
        };
        assert_eq!(
            check_transition(
                11,
                &delta,
                |raw| parent.get(raw).cloned(),
                |raw| post.get(raw).cloned(),
                &reference,
            ),
            Err(GuardError::GenesisInstitutionDeleted)
        );
    }

    #[test]
    fn citizen_identity_uses_permanent_cid_and_current_account_id_binding() {
        let cid = citizen_cid_number("citizen-guard");
        let account = [7u8; 32];
        let post = citizen_state(&cid, account);
        let delta = post
            .iter()
            .map(|(key, value)| (key.clone(), Some(value.clone())))
            .collect();
        assert_eq!(
            check_transition(
                1,
                &delta,
                |_| None,
                |key| post.get(key).cloned(),
                &GenesisReference::default(),
            ),
            Ok(())
        );

        let identity_key = storage_key::voting_identity(&cid);
        let deletion = BTreeMap::from([(identity_key.clone(), None)]);
        let mut without_identity = post.clone();
        without_identity.remove(&identity_key);
        assert_eq!(
            check_transition(
                2,
                &deletion,
                |key| post.get(key).cloned(),
                |key| without_identity.get(key).cloned(),
                &GenesisReference::default(),
            ),
            Err(GuardError::CitizenIdentityDeleted)
        );

        let reverse_key = storage_key::cid_by_account_id(&account);
        let mut wrong_reverse = post.clone();
        wrong_reverse.insert(
            reverse_key.clone(),
            citizen_cid_number("another-citizen").encode(),
        );
        let mismatch = BTreeMap::from([(
            reverse_key,
            wrong_reverse
                .get(&storage_key::cid_by_account_id(&account))
                .cloned(),
        )]);
        assert_eq!(
            check_transition(
                2,
                &mismatch,
                |key| post.get(key).cloned(),
                |key| wrong_reverse.get(key).cloned(),
                &GenesisReference::default(),
            ),
            Err(GuardError::CitizenAccountIdBindingMissing)
        );

        let revision_key = storage_key::binding_revision_by_cid(&cid);
        let mut missing_revision = post.clone();
        missing_revision.remove(&revision_key);
        assert_eq!(
            validate_citizen_identity_binding(&cid, &|key| missing_revision.get(key).cloned()),
            Err(GuardError::CitizenBindingRevisionMissing)
        );

        let mut missing_reverse = post.clone();
        missing_reverse.remove(&storage_key::cid_by_account_id(&account));
        assert_eq!(
            validate_citizen_identity_binding(&cid, &|key| missing_reverse.get(key).cloned()),
            Err(GuardError::CitizenAccountIdBindingMissing)
        );

        let mut zero_revision = post.clone();
        zero_revision.insert(revision_key, 0u64.encode());
        assert_eq!(
            validate_citizen_identity_binding(&cid, &|key| zero_revision.get(key).cloned()),
            Err(GuardError::CitizenBindingRevisionInvalid)
        );
    }

    #[test]
    fn anonymous_person_cid_binds_account_without_voting_identity() {
        // 居民(NATP)匿名 CID:占号即绑账户,无投票身份、空居住地。守卫必须放行——
        // 用户只凭钱包账户即可注册,投票/竞选身份是可选项而非必须。
        // 同时覆盖两处守卫一致性:命名空间放行 CTZN|NATP(is_person_code)+ 身份可选。
        let cid = natp_cid_number("anon-guard");
        let account = [9u8; 32];
        let post = anonymous_citizen_state(&cid, account);

        // 增量路径:占即绑一笔新块,匿名 CID 过 check_transition。
        let delta = post
            .iter()
            .map(|(key, value)| (key.clone(), Some(value.clone())))
            .collect();
        assert_eq!(
            check_transition(
                1,
                &delta,
                |_| None,
                |key| post.get(key).cloned(),
                &GenesisReference::default(),
            ),
            Ok(())
        );

        // 只有登记表、没有账户闭环和 revision 的半状态必须在增量路径 fail-closed。
        let registry_key = storage_key::citizen_registry(&cid);
        let registry_only_delta =
            BTreeMap::from([(registry_key.clone(), post.get(&registry_key).cloned())]);
        let registry_only_state = BTreeMap::from([(
            registry_key.clone(),
            post.get(&registry_key).cloned().expect("registry record"),
        )]);
        assert_eq!(
            check_transition(
                1,
                &registry_only_delta,
                |_| None,
                |key| registry_only_state.get(key).cloned(),
                &GenesisReference::default(),
            ),
            Err(GuardError::CitizenAccountIdBindingMissing)
        );

        // 绑定闭环校验(增量与全量路径共用的咽喉):匿名 CID 无投票身份也放行。
        assert_eq!(
            validate_citizen_identity_binding(&cid, &|key| post.get(key).cloned()),
            Ok(())
        );

        // 同账户 + 同 CID 的 self_occupy 幂等重试会触及正反绑定但不推进
        // revision；块前/块后值完全相同，NodeGuard 必须放行。
        let idempotent_delta = BTreeMap::from([
            (storage_key::account_id_by_cid(&cid), Some(account.encode())),
            (
                storage_key::cid_by_account_id(&account),
                Some(cid.to_vec().encode()),
            ),
        ]);
        assert_eq!(
            check_transition(
                2,
                &idempotent_delta,
                |key| post.get(key).cloned(),
                |key| post.get(key).cloned(),
                &GenesisReference::default(),
            ),
            Ok(())
        );

        // 换绑:匿名 CID 从旧账户换到新账户(admin_rebind / self_rebind 共用的绑定迁移),
        // 无投票身份也必须过守卫。
        let new_account = [10u8; 32];
        let mut rebound = anonymous_citizen_state(&cid, new_account);
        rebound.remove(&storage_key::cid_by_account_id(&account));
        rebound.insert(storage_key::binding_revision_by_cid(&cid), 2u64.encode());
        let rebind_delta = BTreeMap::from([
            (
                storage_key::account_id_by_cid(&cid),
                Some(new_account.encode()),
            ),
            (storage_key::cid_by_account_id(&account), None),
            (
                storage_key::cid_by_account_id(&new_account),
                Some(cid.to_vec().encode()),
            ),
            (
                storage_key::binding_revision_by_cid(&cid),
                Some(2u64.encode()),
            ),
        ]);
        assert_eq!(
            check_transition(
                2,
                &rebind_delta,
                |key| post.get(key).cloned(),
                |key| rebound.get(key).cloned(),
                &GenesisReference::default(),
            ),
            Ok(())
        );

        // 同块连续 A→B→C 在块级 delta 中只呈现 A→C、revision 1→3，必须放行。
        let final_account = [11u8; 32];
        let mut multi_rebound = anonymous_citizen_state(&cid, final_account);
        multi_rebound.remove(&storage_key::cid_by_account_id(&account));
        multi_rebound.insert(storage_key::binding_revision_by_cid(&cid), 3u64.encode());
        let multi_rebind_delta = BTreeMap::from([
            (
                storage_key::account_id_by_cid(&cid),
                Some(final_account.encode()),
            ),
            (storage_key::cid_by_account_id(&account), None),
            (
                storage_key::cid_by_account_id(&final_account),
                Some(cid.to_vec().encode()),
            ),
            (
                storage_key::binding_revision_by_cid(&cid),
                Some(3u64.encode()),
            ),
        ]);
        assert_eq!(
            check_transition(
                2,
                &multi_rebind_delta,
                |key| post.get(key).cloned(),
                |key| multi_rebound.get(key).cloned(),
                &GenesisReference::default(),
            ),
            Ok(())
        );

        // 同块连续 A→B→A 的最终绑定值与父状态相同，但 forward key 确实被触碰且
        // revision 1→3；不能把这种合法的两次换绑误判成 revision-only。
        let mut round_trip = post.clone();
        round_trip.insert(storage_key::binding_revision_by_cid(&cid), 3u64.encode());
        let round_trip_delta = BTreeMap::from([
            (storage_key::account_id_by_cid(&cid), Some(account.encode())),
            (
                storage_key::cid_by_account_id(&account),
                Some(cid.to_vec().encode()),
            ),
            (storage_key::cid_by_account_id(&new_account), None),
            (
                storage_key::binding_revision_by_cid(&cid),
                Some(3u64.encode()),
            ),
        ]);
        assert_eq!(
            check_transition(
                2,
                &round_trip_delta,
                |key| post.get(key).cloned(),
                |key| round_trip.get(key).cloned(),
                &GenesisReference::default(),
            ),
            Ok(())
        );

        // 只换双向账户而不推进 revision 必须 fail-closed。
        let mut stale_revision_delta = rebind_delta.clone();
        stale_revision_delta.remove(&storage_key::binding_revision_by_cid(&cid));
        let mut stale_revision_state = rebound.clone();
        stale_revision_state.insert(storage_key::binding_revision_by_cid(&cid), 1u64.encode());
        assert_eq!(
            check_transition(
                2,
                &stale_revision_delta,
                |key| post.get(key).cloned(),
                |key| stale_revision_state.get(key).cloned(),
                &GenesisReference::default(),
            ),
            Err(GuardError::CitizenBindingRevisionTransitionInvalid)
        );

        // 无换绑/吊销却单独推进 revision 同样拒绝。
        let revision_only = BTreeMap::from([(
            storage_key::binding_revision_by_cid(&cid),
            Some(2u64.encode()),
        )]);
        let mut revision_only_state = post.clone();
        revision_only_state.insert(storage_key::binding_revision_by_cid(&cid), 2u64.encode());
        assert_eq!(
            check_transition(
                2,
                &revision_only,
                |key| post.get(key).cloned(),
                |key| revision_only_state.get(key).cloned(),
                &GenesisReference::default(),
            ),
            Err(GuardError::CitizenBindingRevisionTransitionInvalid)
        );
    }

    #[test]
    fn candidate_identity_requires_active_normal_voter_and_locks_identity_fields() {
        let cid = citizen_cid_number("candidate-guard");
        let account = [12u8; 32];
        let parent = citizen_state(&cid, account);
        let candidate_key = storage_key::candidate_identity(&cid);
        let mut created = parent.clone();
        created.insert(candidate_key.clone(), candidate_identity(2).encode());
        let creation_delta =
            BTreeMap::from([(candidate_key.clone(), created.get(&candidate_key).cloned())]);
        assert_eq!(
            check_transition(
                2,
                &creation_delta,
                |key| parent.get(key).cloned(),
                |key| created.get(key).cloned(),
                &GenesisReference::default(),
            ),
            Ok(())
        );
        assert!(
            storage_key::enumerated_prefixes().contains(&storage_key::candidate_identity_prefix())
        );
        assert_eq!(
            validate_candidate_identity(&cid, &|key| created.get(key).cloned()),
            Ok(())
        );

        // 只有普通 CID 绑定、没有投票身份时，不能凭空生成竞选身份。
        let mut without_voting = anonymous_citizen_state(&cid, account);
        without_voting.insert(candidate_key.clone(), candidate_identity(2).encode());
        assert_eq!(
            validate_candidate_identity(&cid, &|key| without_voting.get(key).cloned()),
            Err(GuardError::CitizenCandidateIdentityWithoutVotingIdentity)
        );

        // 候选人的出生区划、性别和出生日期一经写入不得改变。
        let mut changed_locked = created.clone();
        let mut changed = candidate_identity(3);
        changed.birth_date = 20010101;
        changed_locked.insert(candidate_key.clone(), changed.encode());
        let locked_delta = BTreeMap::from([(
            candidate_key.clone(),
            changed_locked.get(&candidate_key).cloned(),
        )]);
        assert_eq!(
            check_transition(
                3,
                &locked_delta,
                |key| created.get(key).cloned(),
                |key| changed_locked.get(key).cloned(),
                &GenesisReference::default(),
            ),
            Err(GuardError::CitizenCandidateIdentityLockedFieldChanged)
        );

        // 姓名可更新，但 updated_at 必须等于发生变化的当前区块。
        let mut renamed = created.clone();
        let mut renamed_identity = candidate_identity(3);
        renamed_identity.family_name = b"Updated".to_vec();
        renamed.insert(candidate_key.clone(), renamed_identity.encode());
        let rename_delta =
            BTreeMap::from([(candidate_key.clone(), renamed.get(&candidate_key).cloned())]);
        assert_eq!(
            check_transition(
                3,
                &rename_delta,
                |key| created.get(key).cloned(),
                |key| renamed.get(key).cloned(),
                &GenesisReference::default(),
            ),
            Ok(())
        );
        assert_eq!(
            check_transition(
                4,
                &rename_delta,
                |key| created.get(key).cloned(),
                |key| renamed.get(key).cloned(),
                &GenesisReference::default(),
            ),
            Err(GuardError::CitizenCandidateIdentityUpdateHeightInvalid)
        );

        // SCALE 值必须精确消费，尾随字节同样拒绝。
        let mut malformed = created.clone();
        malformed
            .get_mut(&candidate_key)
            .expect("candidate identity")
            .push(0xff);
        assert_eq!(
            validate_candidate_identity(&cid, &|key| malformed.get(key).cloned()),
            Err(GuardError::StorageValueDecodeFailed(
                "CandidateIdentityByCid"
            ))
        );
    }

    #[test]
    fn candidate_identity_can_only_be_deleted_with_cid_and_voting_revocation() {
        let cid = citizen_cid_number("candidate-revoke");
        let account = [13u8; 32];
        let candidate_key = storage_key::candidate_identity(&cid);
        let mut parent = citizen_state(&cid, account);
        parent.insert(candidate_key.clone(), candidate_identity(2).encode());

        let mut illegal = parent.clone();
        illegal.remove(&candidate_key);
        let illegal_delta = BTreeMap::from([(candidate_key.clone(), None)]);
        assert_eq!(
            check_transition(
                3,
                &illegal_delta,
                |key| parent.get(key).cloned(),
                |key| illegal.get(key).cloned(),
                &GenesisReference::default(),
            ),
            Err(GuardError::CitizenCandidateIdentityDeleted)
        );

        let registry_key = storage_key::citizen_registry(&cid);
        let voting_key = storage_key::voting_identity(&cid);
        let revision_key = storage_key::binding_revision_by_cid(&cid);
        let mut revoked = illegal;
        let mut registry: CitizenCidRecord = decode_exact(
            revoked.get(&registry_key).expect("cid registry"),
            "CidRegistry",
        )
        .expect("registry decodes");
        registry.status = CitizenCidStatus::Revoked;
        registry.revoked_at = Some(3);
        revoked.insert(registry_key.clone(), registry.encode());
        let mut voting: CitizenVotingIdentity = decode_exact(
            revoked.get(&voting_key).expect("voting identity"),
            "VotingIdentityByCid",
        )
        .expect("voting identity decodes");
        voting.citizen_status = CitizenStatus::Revoked;
        voting.updated_at = 3;
        revoked.insert(voting_key.clone(), voting.encode());
        revoked.insert(revision_key.clone(), 2u64.encode());
        let lawful_delta = BTreeMap::from([
            (
                registry_key,
                revoked.get(&storage_key::citizen_registry(&cid)).cloned(),
            ),
            (
                voting_key,
                revoked.get(&storage_key::voting_identity(&cid)).cloned(),
            ),
            (candidate_key, None),
            (revision_key, Some(2u64.encode())),
        ]);
        assert_eq!(
            check_transition(
                3,
                &lawful_delta,
                |key| parent.get(key).cloned(),
                |key| revoked.get(key).cloned(),
                &GenesisReference::default(),
            ),
            Ok(())
        );
    }
}

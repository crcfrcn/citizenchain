use codec::Decode;
use codec::Encode;
use primitives::account_derive::institution_kind_by_name;
use primitives::account_derive::institution_protocol_account_name;
use primitives::account_derive::institution_protocol_kind_by_name;
use primitives::account_derive::InstitutionProtocolAccountKind;
use primitives::cid::code::institution_code_from_str;
use primitives::cid::code::is_fixed_governance_code;
use primitives::cid::code::is_person_code;
use primitives::cid::code::is_private_legal_code;
use primitives::cid::code::is_public_legal_code;
use primitives::cid::code::is_three_char_code;
use primitives::cid::code::is_unincorporated_code;
use primitives::cid::code::profit_policy;
use primitives::cid::code::province_code_text;
use primitives::cid::code::InstitutionCode;
use primitives::cid::code::ProfitPolicy;
use primitives::core_const::SS58_FORMAT;
use sp_crypto_hashing::blake2_128;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

const CITIZEN_IDENTITY_PALLET: &[u8] = b"CitizenIdentity";

const PUBLIC_MANAGE_PALLET: &[u8] = b"PublicManage";

const PRIVATE_MANAGE_PALLET: &[u8] = b"PrivateManage";

const CODE_KEY: &[u8] = b":code";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Namespace {
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
pub(crate) enum CitizenCidStatus {
    Active,
    Revoked,
}

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, PartialEq)]
pub(crate) enum CitizenStatus {
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
pub(crate) struct GenesisReference {
    institutions: BTreeMap<(u8, Vec<u8>), GenesisInstitutionIdentity>,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum GuardError {
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

fn decode_exact<T: Decode>(raw: &[u8], label: &'static str) -> Result<T, GuardError> {
    let mut input = raw;
    let value = T::decode(&mut input)
        .map_err(|_| crate::guard::cid_lifecycle::GuardError::StorageValueDecodeFailed(label))?;
    if !input.is_empty() {
        return Err(crate::guard::cid_lifecycle::GuardError::StorageValueDecodeFailed(label));
    }
    Ok(value)
}

fn parse_vec_map_key(
    key: &[u8],
    prefix: &[u8],
    label: &'static str,
) -> Result<Vec<u8>, GuardError> {
    if !key.starts_with(prefix) || key.len() < prefix.len() + 17 {
        return Err(crate::guard::cid_lifecycle::GuardError::StorageKeyMalformed(label));
    }
    let hash_at = prefix.len();
    let encoded_at = hash_at + 16;
    let encoded = &key[encoded_at..];
    if blake2_128(encoded) != key[hash_at..encoded_at] {
        return Err(crate::guard::cid_lifecycle::GuardError::StorageKeyMalformed(label));
    }
    decode_exact(encoded, label)
}

fn parse_account_map_key(
    key: &[u8],
    prefix: &[u8],
    label: &'static str,
) -> Result<[u8; 32], GuardError> {
    if !key.starts_with(prefix) || key.len() != prefix.len() + 16 + 32 {
        return Err(crate::guard::cid_lifecycle::GuardError::StorageKeyMalformed(label));
    }
    let hash_at = prefix.len();
    let account_at = hash_at + 16;
    if blake2_128(&key[account_at..]) != key[hash_at..account_at] {
        return Err(crate::guard::cid_lifecycle::GuardError::StorageKeyMalformed(label));
    }
    key[account_at..]
        .try_into()
        .map_err(|_| crate::guard::cid_lifecycle::GuardError::StorageKeyMalformed(label))
}

fn parse_double_vec_key(
    key: &[u8],
    prefix: &[u8],
    label: &'static str,
) -> Result<(Vec<u8>, Vec<u8>), GuardError> {
    if !key.starts_with(prefix) || key.len() < prefix.len() + 34 {
        return Err(crate::guard::cid_lifecycle::GuardError::StorageKeyMalformed(label));
    }
    let first_hash_at = prefix.len();
    let first_encoded_at = first_hash_at + 16;
    let first_encoded = &key[first_encoded_at..];
    let mut input = first_encoded;
    let first = Vec::<u8>::decode(&mut input)
        .map_err(|_| crate::guard::cid_lifecycle::GuardError::StorageKeyMalformed(label))?;
    let first_encoded_len = first_encoded.len() - input.len();
    if blake2_128(&first_encoded[..first_encoded_len]) != key[first_hash_at..first_encoded_at] {
        return Err(crate::guard::cid_lifecycle::GuardError::StorageKeyMalformed(label));
    }
    let second_hash_at = first_encoded_at + first_encoded_len;
    if key.len() < second_hash_at + 17 {
        return Err(crate::guard::cid_lifecycle::GuardError::StorageKeyMalformed(label));
    }
    let second_encoded_at = second_hash_at + 16;
    let second_encoded = &key[second_encoded_at..];
    if blake2_128(second_encoded) != key[second_hash_at..second_encoded_at] {
        return Err(crate::guard::cid_lifecycle::GuardError::StorageKeyMalformed(label));
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
        return Err(crate::guard::cid_lifecycle::GuardError::InvalidCid);
    }
    let r5 = segments[0];
    let core = segments[1];
    if !r5.iter().all(u8::is_ascii_alphanumeric)
        || !core.iter().all(u8::is_ascii_alphanumeric)
        || r5.iter().any(u8::is_ascii_lowercase)
        || core.iter().any(u8::is_ascii_lowercase)
    {
        return Err(crate::guard::cid_lifecycle::GuardError::InvalidCid);
    }

    let (code_text, profit, checksum, expected_checksum) = if core[3].is_ascii_digit() {
        let code_text = core::str::from_utf8(&core[..3])
            .map_err(|_| crate::guard::cid_lifecycle::GuardError::InvalidCid)?;
        let profit = match core[3] {
            b'0' => false,
            b'1' => true,
            _ => return Err(crate::guard::cid_lifecycle::GuardError::InvalidCid),
        };
        let total = checksum_acc(&[r5, &core[..4], segments[2], segments[3]]);
        let expected = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"[total % 36];
        (code_text, profit, core[4], expected)
    } else {
        let code_text = core::str::from_utf8(&core[..4])
            .map_err(|_| crate::guard::cid_lifecycle::GuardError::InvalidCid)?;
        let profit = core[4].is_ascii_digit();
        if !profit && !core[4].is_ascii_uppercase() {
            return Err(crate::guard::cid_lifecycle::GuardError::InvalidCid);
        }
        let total = checksum_acc(&[r5, &core[..4], segments[2], segments[3]]);
        let expected = if profit {
            b'0' + (total % 10) as u8
        } else {
            b'A' + (total % 26) as u8
        };
        (code_text, profit, core[4], expected)
    };

    let code = institution_code_from_str(code_text)
        .ok_or(crate::guard::cid_lifecycle::GuardError::InvalidCid)?;
    if is_three_char_code(&code) != (code_text.len() == 3) || checksum != expected_checksum {
        return Err(crate::guard::cid_lifecycle::GuardError::InvalidCid);
    }
    match profit_policy(&code) {
        Some(ProfitPolicy::NonProfit) if profit => {
            return Err(crate::guard::cid_lifecycle::GuardError::InvalidCid)
        }
        Some(ProfitPolicy::Profit) if !profit => {
            return Err(crate::guard::cid_lifecycle::GuardError::InvalidCid)
        }
        Some(_) => {}
        None => return Err(crate::guard::cid_lifecycle::GuardError::InvalidCid),
    }
    if !valid_province_city(code, r5) {
        return Err(crate::guard::cid_lifecycle::GuardError::InvalidCid);
    }
    Ok(code)
}

fn validate_cid_namespace(cid: &[u8], namespace: Namespace) -> Result<InstitutionCode, GuardError> {
    let code = parse_cid(cid)?;
    let valid = match namespace {
        crate::guard::cid_lifecycle::Namespace::Public => {
            is_public_legal_code(&code) || is_fixed_governance_code(&code)
        }
        crate::guard::cid_lifecycle::Namespace::Private => {
            is_private_legal_code(&code) || is_unincorporated_code(&code)
        }
    };
    if !valid {
        return Err(crate::guard::cid_lifecycle::GuardError::InvalidCidNamespace);
    }
    if is_fixed_governance_code(&code)
        && primitives::governance_skeleton::fixed_institution_by_cid(cid)
            .is_none_or(|institution| institution.code != code)
    {
        return Err(crate::guard::cid_lifecycle::GuardError::InstitutionIdentityChanged);
    }
    if primitives::institution_constraints::is_permanent_singleton_code(&code)
        && primitives::institution_constraints::singleton_by_cid(cid)
            .is_none_or(|institution| institution.code != code)
    {
        return Err(crate::guard::cid_lifecycle::GuardError::SingletonInstitutionIdentityMismatch);
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
        return Err(crate::guard::cid_lifecycle::GuardError::InvalidCidNamespace);
    }
    match record.status {
        crate::guard::cid_lifecycle::CitizenCidStatus::Active if record.revoked_at.is_none() => {}
        crate::guard::cid_lifecycle::CitizenCidStatus::Revoked if record.revoked_at.is_some() => {}
        _ => return Err(crate::guard::cid_lifecycle::GuardError::CitizenCidStatusInvalid),
    }
    if let Some(block) = block {
        if record.registered_at > block || record.revoked_at.is_some_and(|at| at > block) {
            return Err(crate::guard::cid_lifecycle::GuardError::CitizenCidRevocationHeightInvalid);
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
        return Err(crate::guard::cid_lifecycle::GuardError::InstitutionIdentityChanged);
    }
    if let Some(representative) = &record.legal_representative {
        if representative.family_name.is_empty()
            || representative.given_name.is_empty()
            || representative.cid_number.is_empty()
        {
            return Err(
                crate::guard::cid_lifecycle::GuardError::InstitutionLegalRepresentativeInvalid,
            );
        }
    }
    Ok(())
}

fn validate_account_record(
    cid: &[u8],
    name: &[u8],
    record: &InstitutionAccountRecord,
) -> Result<(), GuardError> {
    let kind = institution_kind_by_name(cid, name)
        .ok_or(crate::guard::cid_lifecycle::GuardError::AccountAddressMismatch)?;
    if kind.derive(SS58_FORMAT) != record.address {
        return Err(crate::guard::cid_lifecycle::GuardError::AccountAddressMismatch);
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
    let reverse_raw = read(
        &crate::guard::cid_lifecycle::storage_key::account_registered(namespace, &record.address),
    )
    .ok_or(crate::guard::cid_lifecycle::GuardError::AccountReverseIndexMissing)?;
    let reverse: RegisteredInstitution = decode_exact(&reverse_raw, "AccountRegisteredCid")?;
    if reverse.cid_number != cid || reverse.account_name != name {
        return Err(crate::guard::cid_lifecycle::GuardError::AccountReverseIndexMismatch);
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
    .ok_or(crate::guard::cid_lifecycle::GuardError::InstitutionIdentityChanged)?;
    for kind in required {
        let name = institution_protocol_account_name(*kind);
        let raw = read(
            &crate::guard::cid_lifecycle::storage_key::institution_account_id(namespace, cid, name),
        )
        .ok_or(crate::guard::cid_lifecycle::GuardError::RequiredProtocolAccountMissing)?;
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
    let post_raw = post_raw.ok_or(crate::guard::cid_lifecycle::GuardError::CitizenCidDeleted)?;
    let post: CitizenCidRecord = decode_exact(&post_raw, "CidRegistry")?;
    validate_citizen_record(cid, &post, Some(block))?;
    let Some(parent_raw) = parent_raw else {
        if post.registered_at != block
            || (post.status == crate::guard::cid_lifecycle::CitizenCidStatus::Revoked
                && post.revoked_at != Some(block))
        {
            return Err(crate::guard::cid_lifecycle::GuardError::CitizenCidRevocationHeightInvalid);
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
        return Err(crate::guard::cid_lifecycle::GuardError::CitizenCidIdentityChanged);
    }
    match (parent.status, post.status) {
        (
            crate::guard::cid_lifecycle::CitizenCidStatus::Active,
            crate::guard::cid_lifecycle::CitizenCidStatus::Active,
        ) if parent_raw == post_raw => Ok(()),
        (
            crate::guard::cid_lifecycle::CitizenCidStatus::Active,
            crate::guard::cid_lifecycle::CitizenCidStatus::Revoked,
        ) if post.revoked_at == Some(block) => Ok(()),
        (
            crate::guard::cid_lifecycle::CitizenCidStatus::Revoked,
            crate::guard::cid_lifecycle::CitizenCidStatus::Revoked,
        ) if parent_raw == post_raw => Ok(()),
        _ => Err(crate::guard::cid_lifecycle::GuardError::CitizenCidStatusInvalid),
    }
}

fn validate_candidate_identity<F>(cid: &[u8], read: &F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let Some(candidate_raw) =
        read(&crate::guard::cid_lifecycle::storage_key::candidate_identity(cid))
    else {
        return Ok(());
    };
    let _: CitizenCandidateIdentity = decode_exact(&candidate_raw, "CandidateIdentityByCid")?;
    let registry_raw = read(&crate::guard::cid_lifecycle::storage_key::citizen_registry(
        cid,
    ))
    .ok_or(crate::guard::cid_lifecycle::GuardError::CitizenIdentityWithoutCid)?;
    let registry: CitizenCidRecord = decode_exact(&registry_raw, "CidRegistry")?;
    if registry.status != crate::guard::cid_lifecycle::CitizenCidStatus::Active {
        return Err(crate::guard::cid_lifecycle::GuardError::CitizenCandidateIdentityInvalidStatus);
    }
    let voting_raw = read(&crate::guard::cid_lifecycle::storage_key::voting_identity(
        cid,
    ))
    .ok_or(
        crate::guard::cid_lifecycle::GuardError::CitizenCandidateIdentityWithoutVotingIdentity,
    )?;
    let voting: CitizenVotingIdentity = decode_exact(&voting_raw, "VotingIdentityByCid")?;
    if voting.citizen_status != crate::guard::cid_lifecycle::CitizenStatus::Normal {
        return Err(crate::guard::cid_lifecycle::GuardError::CitizenCandidateIdentityInvalidStatus);
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
                return Err(crate::guard::cid_lifecycle::GuardError::CitizenCandidateIdentityUpdateHeightInvalid);
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
                return Err(crate::guard::cid_lifecycle::GuardError::CitizenCandidateIdentityLockedFieldChanged);
            }
            if before_raw != after_raw && after.updated_at != block {
                return Err(crate::guard::cid_lifecycle::GuardError::CitizenCandidateIdentityUpdateHeightInvalid);
            }
            validate_candidate_identity(cid, post)
        }
        (Some(_), None) => {
            let registry_raw = post(&crate::guard::cid_lifecycle::storage_key::citizen_registry(
                cid,
            ))
            .ok_or(crate::guard::cid_lifecycle::GuardError::CitizenCandidateIdentityDeleted)?;
            let registry: CitizenCidRecord = decode_exact(&registry_raw, "CidRegistry")?;
            let voting_raw = post(&crate::guard::cid_lifecycle::storage_key::voting_identity(
                cid,
            ))
            .ok_or(crate::guard::cid_lifecycle::GuardError::CitizenCandidateIdentityDeleted)?;
            let voting: CitizenVotingIdentity = decode_exact(&voting_raw, "VotingIdentityByCid")?;
            if registry.status != crate::guard::cid_lifecycle::CitizenCidStatus::Revoked
                || voting.citizen_status != crate::guard::cid_lifecycle::CitizenStatus::Revoked
            {
                return Err(
                    crate::guard::cid_lifecycle::GuardError::CitizenCandidateIdentityDeleted,
                );
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
    let registry_raw = read(&crate::guard::cid_lifecycle::storage_key::citizen_registry(
        cid,
    ))
    .ok_or(crate::guard::cid_lifecycle::GuardError::CitizenIdentityWithoutCid)?;
    let registry: CitizenCidRecord = decode_exact(&registry_raw, "CidRegistry")?;
    validate_citizen_record(cid, &registry, None)?;

    // 投票身份、竞选身份是用户可选项,不是必须:匿名 CID(占号即绑账户、无投票身份)
    // 合法。存在则校验可解码,不存在则放行——守卫不强制任何身份信息,用户只凭钱包账户
    // 即可占号绑定。投票身份一旦创建不得删除的单调性由 `check_transition` 单独兜底。
    if let Some(identity_raw) = read(&crate::guard::cid_lifecycle::storage_key::voting_identity(
        cid,
    )) {
        let _: CitizenVotingIdentity = decode_exact(&identity_raw, "VotingIdentityByCid")?;
    }
    validate_candidate_identity(cid, read)?;

    let account_raw = read(&crate::guard::cid_lifecycle::storage_key::account_id_by_cid(cid))
        .ok_or(crate::guard::cid_lifecycle::GuardError::CitizenAccountIdBindingMissing)?;
    let account: [u8; 32] = decode_exact(&account_raw, "AccountIdByCid")?;
    let reverse_raw = read(&crate::guard::cid_lifecycle::storage_key::cid_by_account_id(&account))
        .ok_or(crate::guard::cid_lifecycle::GuardError::CitizenAccountIdBindingMissing)?;
    let reverse_cid: Vec<u8> = decode_exact(&reverse_raw, "CidByAccountId")?;
    if reverse_cid != cid {
        return Err(crate::guard::cid_lifecycle::GuardError::CitizenAccountIdBindingMismatch);
    }
    let revision_raw =
        read(&crate::guard::cid_lifecycle::storage_key::binding_revision_by_cid(cid))
            .ok_or(crate::guard::cid_lifecycle::GuardError::CitizenBindingRevisionMissing)?;
    let revision: u64 = decode_exact(&revision_raw, "BindingRevisionByCid")?;
    if revision == 0 {
        return Err(crate::guard::cid_lifecycle::GuardError::CitizenBindingRevisionInvalid);
    }
    Ok(())
}

/// 普通区块只对本块触及的 CID、机构和账户执行单调性与正反索引校验。
pub(crate) fn check_transition<FParent, FPost>(
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
    let citizen_prefix = crate::guard::cid_lifecycle::storage_key::citizen_registry_prefix();
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
            if before.status == crate::guard::cid_lifecycle::CitizenCidStatus::Active
                && after.status == crate::guard::cid_lifecycle::CitizenCidStatus::Revoked
            {
                required_revision_changes.insert(cid);
            }
        }
    }

    let identity_prefix = crate::guard::cid_lifecycle::storage_key::voting_identity_prefix();
    let candidate_prefix = crate::guard::cid_lifecycle::storage_key::candidate_identity_prefix();
    let forward_prefix = crate::guard::cid_lifecycle::storage_key::account_id_by_cid_prefix();
    let reverse_prefix = crate::guard::cid_lifecycle::storage_key::cid_by_account_id_prefix();
    let revision_prefix =
        crate::guard::cid_lifecycle::storage_key::binding_revision_by_cid_prefix();
    let mut forward_touched = BTreeSet::<Vec<u8>>::new();
    for key in delta.keys() {
        if key.starts_with(&identity_prefix) {
            let cid = parse_vec_map_key(key, &identity_prefix, "VotingIdentityByCid")?;
            if post(key).is_none() {
                return Err(crate::guard::cid_lifecycle::GuardError::CitizenIdentityDeleted);
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
                let forward_raw =
                    post(&crate::guard::cid_lifecycle::storage_key::account_id_by_cid(&cid))
                        .ok_or(
                            crate::guard::cid_lifecycle::GuardError::CitizenAccountIdBindingMissing,
                        )?;
                let forward: [u8; 32] = decode_exact(&forward_raw, "AccountIdByCid")?;
                if forward != account {
                    return Err(
                        crate::guard::cid_lifecycle::GuardError::CitizenAccountIdBindingMismatch,
                    );
                }
                touched_citizens.insert(cid);
            } else if let Some(raw) = parent(key) {
                let cid: Vec<u8> = decode_exact(&raw, "CidByAccountId")?;
                if post(&crate::guard::cid_lifecycle::storage_key::account_id_by_cid(&cid))
                    .is_some_and(|raw| {
                        crate::guard::cid_lifecycle::decode_exact::<[u8; 32]>(
                            &raw,
                            "AccountIdByCid",
                        ) == Ok(account)
                    })
                {
                    return Err(
                        crate::guard::cid_lifecycle::GuardError::CitizenAccountIdBindingMismatch,
                    );
                }
                touched_citizens.insert(cid);
            }
        }
    }
    for key in delta.keys().filter(|key| key.starts_with(&revision_prefix)) {
        let cid = parse_vec_map_key(key, &revision_prefix, "BindingRevisionByCid")?;
        let after_raw = post(key)
            .ok_or(crate::guard::cid_lifecycle::GuardError::CitizenBindingRevisionMissing)?;
        let after: u64 = decode_exact(&after_raw, "BindingRevisionByCid")?;
        let valid = match parent(key) {
            None => {
                after == 1
                    && parent(&crate::guard::cid_lifecycle::storage_key::account_id_by_cid(&cid))
                        .is_none()
                    && post(&crate::guard::cid_lifecycle::storage_key::account_id_by_cid(&cid))
                        .is_some()
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
            return Err(
                crate::guard::cid_lifecycle::GuardError::CitizenBindingRevisionTransitionInvalid,
            );
        }
        required_revision_changes.remove(&cid);
        touched_citizens.insert(cid);
    }
    if !required_revision_changes.is_empty() {
        return Err(
            crate::guard::cid_lifecycle::GuardError::CitizenBindingRevisionTransitionInvalid,
        );
    }
    for cid in touched_citizens {
        validate_citizen_identity_binding(&cid, &post)?;
    }

    let mut touched_institutions = BTreeSet::<(u8, Vec<u8>)>::new();
    for namespace in [
        crate::guard::cid_lifecycle::Namespace::Public,
        crate::guard::cid_lifecycle::Namespace::Private,
    ] {
        let namespace_id = if namespace == crate::guard::cid_lifecycle::Namespace::Public {
            0
        } else {
            1
        };
        let institution_prefix =
            crate::guard::cid_lifecycle::storage_key::institution_prefix(namespace);
        let account_prefix =
            crate::guard::cid_lifecycle::storage_key::institution_account_prefix(namespace);
        let reverse_prefix =
            crate::guard::cid_lifecycle::storage_key::account_registered_prefix(namespace);

        for key in delta.keys() {
            if key.starts_with(&institution_prefix) {
                let cid = parse_vec_map_key(key, &institution_prefix, "Institutions")?;
                let protected = reference.institutions.get(&(namespace_id, cid.clone()));
                let Some(post_raw) = post(key) else {
                    if protected.is_some() {
                        return Err(
                            crate::guard::cid_lifecycle::GuardError::GenesisInstitutionDeleted,
                        );
                    }
                    // 普通机构可以由 runtime 依法删除；关联账户和索引由后续全状态检查
                    // 确认已经同时清空，NodeGuard 不再把普通主体永久化。
                    continue;
                };
                let post_record: InstitutionRecord = decode_exact(&post_raw, "Institutions")?;
                validate_institution_record(namespace, &cid, &post_record)?;
                if post(&crate::guard::cid_lifecycle::storage_key::institution(
                    namespace.sibling(),
                    &cid,
                ))
                .is_some()
                {
                    return Err(crate::guard::cid_lifecycle::GuardError::CrossNamespaceDuplicate);
                }
                if parent(key).is_some() {
                    if protected.is_some_and(|identity| {
                        identity.institution_code != post_record.institution_code
                            || identity.created_at != post_record.created_at
                            || identity.town_code != post_record.town_code
                    }) {
                        return Err(
                            crate::guard::cid_lifecycle::GuardError::GenesisInstitutionChanged,
                        );
                    }
                } else if is_fixed_governance_code(&post_record.institution_code)
                    || primitives::institution_constraints::is_permanent_singleton_code(
                        &post_record.institution_code,
                    )
                {
                    return Err(crate::guard::cid_lifecycle::GuardError::FixedInstitutionCreatedAfterGenesis);
                }
                touched_institutions.insert((namespace_id, cid));
            }

            if key.starts_with(&account_prefix) {
                let (cid, name) =
                    parse_double_vec_key(key, &account_prefix, "InstitutionAccounts")?;
                let institution = post(&crate::guard::cid_lifecycle::storage_key::institution(
                    namespace, &cid,
                ))
                .map(|raw| {
                    crate::guard::cid_lifecycle::decode_exact::<InstitutionRecord>(
                        &raw,
                        "Institutions",
                    )
                })
                .transpose()?;
                if let Some(institution) = &institution {
                    validate_institution_record(namespace, &cid, institution)?;
                }
                match (parent(key), post(key)) {
                    (Some(before), Some(after)) if before == after => {
                        if institution.is_none() {
                            return Err(
                                crate::guard::cid_lifecycle::GuardError::AccountWithoutInstitution,
                            );
                        }
                    }
                    (Some(_), Some(_)) => {
                        return Err(crate::guard::cid_lifecycle::GuardError::AccountChanged)
                    }
                    (Some(before), None) => {
                        let account: InstitutionAccountRecord =
                            decode_exact(&before, "InstitutionAccounts")?;
                        if institution.is_some() {
                            let kind = institution_kind_by_name(&cid, &name).ok_or(
                                crate::guard::cid_lifecycle::GuardError::AccountAddressMismatch,
                            )?;
                            if !kind.is_closable_institution_account() {
                                return Err(
                                    crate::guard::cid_lifecycle::GuardError::ProtocolAccountDeleted,
                                );
                            }
                        }
                        if post(
                            &crate::guard::cid_lifecycle::storage_key::account_registered(
                                namespace,
                                &account.address,
                            ),
                        )
                        .is_some()
                        {
                            return Err(crate::guard::cid_lifecycle::GuardError::AccountReverseIndexMismatch);
                        }
                    }
                    (None, Some(after)) => {
                        let institution = institution.as_ref().ok_or(
                            crate::guard::cid_lifecycle::GuardError::AccountWithoutInstitution,
                        )?;
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
                                .ok_or(crate::guard::cid_lifecycle::GuardError::InstitutionIdentityChanged)?;
                            if !allowed.contains(&protocol_kind) {
                                return Err(crate::guard::cid_lifecycle::GuardError::UnexpectedProtocolAccount);
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
                    let forward_raw = post(
                        &crate::guard::cid_lifecycle::storage_key::institution_account_id(
                            namespace,
                            &registered.cid_number,
                            &registered.account_name,
                        ),
                    )
                    .ok_or(crate::guard::cid_lifecycle::GuardError::AccountReverseIndexMismatch)?;
                    let forward: InstitutionAccountRecord =
                        decode_exact(&forward_raw, "InstitutionAccounts")?;
                    if forward.address != account {
                        return Err(
                            crate::guard::cid_lifecycle::GuardError::AccountReverseIndexMismatch,
                        );
                    }
                    touched_institutions.insert((namespace_id, registered.cid_number));
                }
            }
        }
    }

    for (namespace_id, cid) in touched_institutions {
        let namespace = if namespace_id == 0 {
            crate::guard::cid_lifecycle::Namespace::Public
        } else {
            crate::guard::cid_lifecycle::Namespace::Private
        };
        let raw = post(&crate::guard::cid_lifecycle::storage_key::institution(
            namespace, &cid,
        ))
        .ok_or(crate::guard::cid_lifecycle::GuardError::AccountWithoutInstitution)?;
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
    let citizen_prefix = crate::guard::cid_lifecycle::storage_key::citizen_registry_prefix();
    for key in keys.iter().filter(|key| key.starts_with(&citizen_prefix)) {
        let Some(raw) = read(key) else { continue };
        let cid = parse_vec_map_key(key, &citizen_prefix, "CidRegistry")?;
        let record: CitizenCidRecord = decode_exact(&raw, "CidRegistry")?;
        validate_citizen_record(&cid, &record, None)?;
        // 每个 Active/Revoked CID 都必须存在唯一账户正反绑定和非零 revision；
        // 仅有登记表记录的半状态同样拒绝。
        validate_citizen_identity_binding(&cid, read)?;
    }

    let identity_prefix = crate::guard::cid_lifecycle::storage_key::voting_identity_prefix();
    for key in keys.iter().filter(|key| key.starts_with(&identity_prefix)) {
        let Some(_) = read(key) else { continue };
        let cid = parse_vec_map_key(key, &identity_prefix, "VotingIdentityByCid")?;
        validate_citizen_identity_binding(&cid, read)?;
    }

    let candidate_prefix = crate::guard::cid_lifecycle::storage_key::candidate_identity_prefix();
    for key in keys.iter().filter(|key| key.starts_with(&candidate_prefix)) {
        let Some(_) = read(key) else { continue };
        let cid = parse_vec_map_key(key, &candidate_prefix, "CandidateIdentityByCid")?;
        validate_citizen_identity_binding(&cid, read)?;
    }

    let forward_prefix = crate::guard::cid_lifecycle::storage_key::account_id_by_cid_prefix();
    for key in keys.iter().filter(|key| key.starts_with(&forward_prefix)) {
        let Some(_) = read(key) else { continue };
        let cid = parse_vec_map_key(key, &forward_prefix, "AccountIdByCid")?;
        validate_citizen_identity_binding(&cid, read)?;
    }

    let reverse_prefix = crate::guard::cid_lifecycle::storage_key::cid_by_account_id_prefix();
    for key in keys.iter().filter(|key| key.starts_with(&reverse_prefix)) {
        let Some(raw) = read(key) else { continue };
        let account = parse_account_map_key(key, &reverse_prefix, "CidByAccountId")?;
        let cid: Vec<u8> = decode_exact(&raw, "CidByAccountId")?;
        let forward_raw = read(&crate::guard::cid_lifecycle::storage_key::account_id_by_cid(&cid))
            .ok_or(crate::guard::cid_lifecycle::GuardError::CitizenAccountIdBindingMissing)?;
        let forward: [u8; 32] = decode_exact(&forward_raw, "AccountIdByCid")?;
        if forward != account {
            return Err(crate::guard::cid_lifecycle::GuardError::CitizenAccountIdBindingMismatch);
        }
        validate_citizen_identity_binding(&cid, read)?;
    }

    let revision_prefix =
        crate::guard::cid_lifecycle::storage_key::binding_revision_by_cid_prefix();
    for key in keys.iter().filter(|key| key.starts_with(&revision_prefix)) {
        let Some(raw) = read(key) else { continue };
        let cid = parse_vec_map_key(key, &revision_prefix, "BindingRevisionByCid")?;
        let revision: u64 = decode_exact(&raw, "BindingRevisionByCid")?;
        if revision == 0 {
            return Err(crate::guard::cid_lifecycle::GuardError::CitizenBindingRevisionInvalid);
        }
        validate_citizen_identity_binding(&cid, read)?;
    }

    let mut occupied_public = BTreeSet::new();
    let mut occupied_private = BTreeSet::new();
    for namespace in [
        crate::guard::cid_lifecycle::Namespace::Public,
        crate::guard::cid_lifecycle::Namespace::Private,
    ] {
        let institution_prefix =
            crate::guard::cid_lifecycle::storage_key::institution_prefix(namespace);
        let account_prefix =
            crate::guard::cid_lifecycle::storage_key::institution_account_prefix(namespace);
        let reverse_prefix =
            crate::guard::cid_lifecycle::storage_key::account_registered_prefix(namespace);
        let occupied = if namespace == crate::guard::cid_lifecycle::Namespace::Public {
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
                return Err(crate::guard::cid_lifecycle::GuardError::AccountWithoutInstitution);
            }
            let record: InstitutionAccountRecord = decode_exact(&raw, "InstitutionAccounts")?;
            validate_account_record(&cid, &name, &record)?;
            validate_account_reverse(namespace, &cid, &name, &record, read)?;
            if let Some(protocol_kind) = institution_protocol_kind_by_name(&name) {
                let institution_raw = read(&crate::guard::cid_lifecycle::storage_key::institution(
                    namespace, &cid,
                ))
                .ok_or(crate::guard::cid_lifecycle::GuardError::AccountWithoutInstitution)?;
                let institution: InstitutionRecord =
                    decode_exact(&institution_raw, "Institutions")?;
                // 同上：用「允许集」，否则会把带清算账户的合法银行分支行判成非法。
                let allowed = primitives::institution_constraints::allowed_protocol_account_kinds(
                    institution.institution_code,
                    &cid,
                )
                .ok_or(crate::guard::cid_lifecycle::GuardError::InstitutionIdentityChanged)?;
                if !allowed.contains(&protocol_kind) {
                    return Err(crate::guard::cid_lifecycle::GuardError::UnexpectedProtocolAccount);
                }
            }
        }

        for key in keys.iter().filter(|key| key.starts_with(&reverse_prefix)) {
            let Some(raw) = read(key) else { continue };
            let account = parse_account_map_key(key, &reverse_prefix, "AccountRegisteredCid")?;
            let registered: RegisteredInstitution = decode_exact(&raw, "AccountRegisteredCid")?;
            let forward_raw = read(
                &crate::guard::cid_lifecycle::storage_key::institution_account_id(
                    namespace,
                    &registered.cid_number,
                    &registered.account_name,
                ),
            )
            .ok_or(crate::guard::cid_lifecycle::GuardError::AccountReverseIndexMismatch)?;
            let forward: InstitutionAccountRecord =
                decode_exact(&forward_raw, "InstitutionAccounts")?;
            if forward.address != account {
                return Err(crate::guard::cid_lifecycle::GuardError::AccountReverseIndexMismatch);
            }
        }
    }

    if occupied_public
        .iter()
        .any(|cid| occupied_private.contains(cid))
    {
        return Err(crate::guard::cid_lifecycle::GuardError::CrossNamespaceDuplicate);
    }

    if let Some(reference) = reference {
        for ((namespace_id, cid), identity) in &reference.institutions {
            let namespace = if *namespace_id == crate::guard::cid_lifecycle::Namespace::Public.id()
            {
                crate::guard::cid_lifecycle::Namespace::Public
            } else {
                crate::guard::cid_lifecycle::Namespace::Private
            };
            let raw = read(&crate::guard::cid_lifecycle::storage_key::institution(
                namespace, cid,
            ))
            .ok_or(crate::guard::cid_lifecycle::GuardError::GenesisInstitutionDeleted)?;
            let record: InstitutionRecord = decode_exact(&raw, "Institutions")?;
            if record.institution_code != identity.institution_code
                || record.created_at != identity.created_at
                || record.town_code != identity.town_code
            {
                return Err(crate::guard::cid_lifecycle::GuardError::GenesisInstitutionChanged);
            }
            if read(&crate::guard::cid_lifecycle::storage_key::institution(
                namespace.sibling(),
                cid,
            ))
            .is_some()
            {
                return Err(crate::guard::cid_lifecycle::GuardError::CrossNamespaceDuplicate);
            }
        }
    }

    for singleton in primitives::institution_constraints::singleton_institutions() {
        let cid = singleton.cid_number.as_bytes();
        let raw = read(&crate::guard::cid_lifecycle::storage_key::institution(
            crate::guard::cid_lifecycle::Namespace::Public,
            cid,
        ))
        .ok_or(crate::guard::cid_lifecycle::GuardError::SingletonInstitutionMissing)?;
        let record: InstitutionRecord = decode_exact(&raw, "Institutions")?;
        if record.institution_code != singleton.code {
            return Err(
                crate::guard::cid_lifecycle::GuardError::SingletonInstitutionIdentityMismatch,
            );
        }
        let main_raw = read(
            &crate::guard::cid_lifecycle::storage_key::institution_account_id(
                crate::guard::cid_lifecycle::Namespace::Public,
                cid,
                institution_protocol_account_name(InstitutionProtocolAccountKind::Main),
            ),
        )
        .ok_or(crate::guard::cid_lifecycle::GuardError::SingletonInstitutionMissing)?;
        let main: InstitutionAccountRecord = decode_exact(&main_raw, "InstitutionAccounts")?;
        if main.address != singleton.main_account {
            return Err(
                crate::guard::cid_lifecycle::GuardError::SingletonInstitutionIdentityMismatch,
            );
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
        for namespace in [
            crate::guard::cid_lifecycle::Namespace::Public,
            crate::guard::cid_lifecycle::Namespace::Private,
        ] {
            let prefix = crate::guard::cid_lifecycle::storage_key::institution_prefix(namespace);
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

pub(crate) fn check_full_state<F>(
    keys: &[Vec<u8>],
    read: F,
    reference: &GenesisReference,
) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    validate_full_state(keys, &read, Some(reference))
}

pub(crate) fn check_imported_genesis<'a, I>(
    pairs: I,
    reference: &GenesisReference,
) -> Result<(), GuardError>
where
    I: IntoIterator<Item = (&'a Vec<u8>, &'a Vec<u8>)>,
{
    let prefixes = crate::guard::cid_lifecycle::storage_key::relevant_prefixes();
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
pub(crate) fn check_state_import_height(_block: u32) -> Result<(), GuardError> {
    Ok(())
}

pub(crate) fn needs_full_check(delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>) -> bool {
    if delta.contains_key(CODE_KEY) {
        return true;
    }
    [
        crate::guard::cid_lifecycle::Namespace::Public,
        crate::guard::cid_lifecycle::Namespace::Private,
    ]
    .into_iter()
    .map(crate::guard::cid_lifecycle::storage_key::institution_prefix)
    .any(|prefix| {
        delta
            .iter()
            .any(|(key, value)| key.starts_with(&prefix) && value.is_none())
    })
}

pub(crate) fn is_relevant_key(key: &[u8]) -> bool {
    matches_relevant_prefixes(
        key,
        &crate::guard::cid_lifecycle::storage_key::relevant_prefixes(),
    )
}

pub(crate) fn matches_relevant_prefixes(key: &[u8], prefixes: &[Vec<u8>]) -> bool {
    prefixes.iter().any(|prefix| key.starts_with(prefix))
}

pub mod storage_key {
    use super::*;
    use crate::guard::cid_lifecycle::Namespace;
    use crate::guard::cid_lifecycle::CITIZEN_IDENTITY_PALLET;

    fn storage_prefix(pallet: &[u8], storage: &[u8]) -> Vec<u8> {
        nodedesktop::storage::prefix(pallet, storage)
    }

    fn map_vec(pallet: &[u8], storage: &[u8], value: &[u8]) -> Vec<u8> {
        nodedesktop::storage::blake2_map(pallet, storage, &value.to_vec().encode())
    }

    fn map_account(pallet: &[u8], storage: &[u8], account: &[u8; 32]) -> Vec<u8> {
        nodedesktop::storage::blake2_map(pallet, storage, account)
    }

    fn double_map_vec(pallet: &[u8], storage: &[u8], first: &[u8], second: &[u8]) -> Vec<u8> {
        nodedesktop::storage::blake2_double_map(
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
            institution_prefix(crate::guard::cid_lifecycle::Namespace::Public),
            institution_prefix(crate::guard::cid_lifecycle::Namespace::Private),
            institution_account_prefix(crate::guard::cid_lifecycle::Namespace::Public),
            institution_account_prefix(crate::guard::cid_lifecycle::Namespace::Private),
            account_registered_prefix(crate::guard::cid_lifecycle::Namespace::Public),
            account_registered_prefix(crate::guard::cid_lifecycle::Namespace::Private),
        ]
    }

    pub fn relevant_prefixes() -> Vec<Vec<u8>> {
        enumerated_prefixes()
    }
}
#[cfg(test)]
#[path = "../tests/lifecycle.rs"]
mod tests;

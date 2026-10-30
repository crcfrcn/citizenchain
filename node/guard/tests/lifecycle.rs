use super::*;

use crate::guard::cid_lifecycle::check_transition;
use crate::guard::cid_lifecycle::checksum_acc;
use crate::guard::cid_lifecycle::decode_exact;
use crate::guard::cid_lifecycle::parse_cid;
use crate::guard::cid_lifecycle::validate_candidate_identity;
use crate::guard::cid_lifecycle::validate_citizen_identity_binding;
use crate::guard::cid_lifecycle::CitizenCandidateIdentity;
use crate::guard::cid_lifecycle::CitizenCidRecord;
use crate::guard::cid_lifecycle::CitizenVotingIdentity;
use crate::guard::cid_lifecycle::GenesisInstitutionIdentity;
use crate::guard::cid_lifecycle::GenesisReference;
use crate::guard::cid_lifecycle::InstitutionRecord;
use std::collections::BTreeMap;

fn cid_with_unchecked_tail(r5: &[u8], code: &[u8], profit: u8, n9: &[u8], year: &[u8]) -> Vec<u8> {
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
            crate::guard::cid_lifecycle::storage_key::citizen_registry(cid),
            CitizenCidRecord {
                registrar_cid_number: b"registrar".to_vec(),
                commitment: [3u8; 32],
                residence_province_code: b"GD".to_vec(),
                residence_city_code: b"001".to_vec(),
                status: crate::guard::cid_lifecycle::CitizenCidStatus::Active,
                registered_at: 1,
                revoked_at: None,
            }
            .encode(),
        ),
        (
            crate::guard::cid_lifecycle::storage_key::voting_identity(cid),
            (
                20260101u32,
                20360101u32,
                crate::guard::cid_lifecycle::CitizenStatus::Normal,
                b"GD".to_vec(),
                b"001".to_vec(),
                Vec::<u8>::new(),
                1u32,
            )
                .encode(),
        ),
        (
            crate::guard::cid_lifecycle::storage_key::account_id_by_cid(cid),
            account.encode(),
        ),
        (
            crate::guard::cid_lifecycle::storage_key::cid_by_account_id(&account),
            cid.to_vec().encode(),
        ),
        (
            crate::guard::cid_lifecycle::storage_key::binding_revision_by_cid(cid),
            1u64.encode(),
        ),
    ])
}

/// 匿名 CID 状态:占号即绑账户(登记记录空居住地 + 账户双向绑定),**无投票身份**。
/// 用于验证守卫接受「用户只提供钱包账户」的匿名注册(身份是可选项、非必须)。
fn anonymous_citizen_state(cid: &[u8], account: [u8; 32]) -> BTreeMap<Vec<u8>, Vec<u8>> {
    BTreeMap::from([
        (
            crate::guard::cid_lifecycle::storage_key::citizen_registry(cid),
            CitizenCidRecord {
                registrar_cid_number: b"registrar".to_vec(),
                commitment: [3u8; 32],
                residence_province_code: Vec::new(),
                residence_city_code: Vec::new(),
                status: crate::guard::cid_lifecycle::CitizenCidStatus::Active,
                registered_at: 1,
                revoked_at: None,
            }
            .encode(),
        ),
        (
            crate::guard::cid_lifecycle::storage_key::account_id_by_cid(cid),
            account.encode(),
        ),
        (
            crate::guard::cid_lifecycle::storage_key::cid_by_account_id(&account),
            cid.to_vec().encode(),
        ),
        (
            crate::guard::cid_lifecycle::storage_key::binding_revision_by_cid(cid),
            1u64.encode(),
        ),
    ])
}

fn candidate_identity(updated_at: u32) -> CitizenCandidateIdentity {
    CitizenCandidateIdentity {
        birth_province_code: b"GD".to_vec(),
        birth_city_code: b"001".to_vec(),
        birth_town_code: b"001001".to_vec(),
        family_name: b"Guard".to_vec(),
        given_name: b"Candidate".to_vec(),
        citizen_sex: crate::guard::cid_lifecycle::CitizenSex::Female,
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
    assert_eq!(
        parse_cid(&bad_city),
        Err(crate::guard::cid_lifecycle::GuardError::InvalidCid)
    );

    let mut bad_checksum = cid;
    bad_checksum[10] = if bad_checksum[10] == b'Z' { b'Y' } else { b'Z' };
    assert_eq!(
        parse_cid(&bad_checksum),
        Err(crate::guard::cid_lifecycle::GuardError::InvalidCid)
    );
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
    let key = crate::guard::cid_lifecycle::storage_key::institution(
        crate::guard::cid_lifecycle::Namespace::Private,
        &cid,
    );
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
            &crate::guard::cid_lifecycle::GenesisReference::default(),
        ),
        Ok(())
    );

    let reference = GenesisReference {
        institutions: BTreeMap::from([(
            (crate::guard::cid_lifecycle::Namespace::Private.id(), cid),
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
        Err(crate::guard::cid_lifecycle::GuardError::GenesisInstitutionDeleted)
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
            &crate::guard::cid_lifecycle::GenesisReference::default(),
        ),
        Ok(())
    );

    let identity_key = crate::guard::cid_lifecycle::storage_key::voting_identity(&cid);
    let deletion = BTreeMap::from([(identity_key.clone(), None)]);
    let mut without_identity = post.clone();
    without_identity.remove(&identity_key);
    assert_eq!(
        check_transition(
            2,
            &deletion,
            |key| post.get(key).cloned(),
            |key| without_identity.get(key).cloned(),
            &crate::guard::cid_lifecycle::GenesisReference::default(),
        ),
        Err(crate::guard::cid_lifecycle::GuardError::CitizenIdentityDeleted)
    );

    let reverse_key = crate::guard::cid_lifecycle::storage_key::cid_by_account_id(&account);
    let mut wrong_reverse = post.clone();
    wrong_reverse.insert(
        reverse_key.clone(),
        citizen_cid_number("another-citizen").encode(),
    );
    let mismatch = BTreeMap::from([(
        reverse_key,
        wrong_reverse
            .get(&crate::guard::cid_lifecycle::storage_key::cid_by_account_id(&account))
            .cloned(),
    )]);
    assert_eq!(
        check_transition(
            2,
            &mismatch,
            |key| post.get(key).cloned(),
            |key| wrong_reverse.get(key).cloned(),
            &crate::guard::cid_lifecycle::GenesisReference::default(),
        ),
        Err(crate::guard::cid_lifecycle::GuardError::CitizenAccountIdBindingMissing)
    );

    let revision_key = crate::guard::cid_lifecycle::storage_key::binding_revision_by_cid(&cid);
    let mut missing_revision = post.clone();
    missing_revision.remove(&revision_key);
    assert_eq!(
        validate_citizen_identity_binding(&cid, &|key| missing_revision.get(key).cloned()),
        Err(crate::guard::cid_lifecycle::GuardError::CitizenBindingRevisionMissing)
    );

    let mut missing_reverse = post.clone();
    missing_reverse.remove(&crate::guard::cid_lifecycle::storage_key::cid_by_account_id(&account));
    assert_eq!(
        validate_citizen_identity_binding(&cid, &|key| missing_reverse.get(key).cloned()),
        Err(crate::guard::cid_lifecycle::GuardError::CitizenAccountIdBindingMissing)
    );

    let mut zero_revision = post.clone();
    zero_revision.insert(revision_key, 0u64.encode());
    assert_eq!(
        validate_citizen_identity_binding(&cid, &|key| zero_revision.get(key).cloned()),
        Err(crate::guard::cid_lifecycle::GuardError::CitizenBindingRevisionInvalid)
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
            &crate::guard::cid_lifecycle::GenesisReference::default(),
        ),
        Ok(())
    );

    // 只有登记表、没有账户闭环和 revision 的半状态必须在增量路径 fail-closed。
    let registry_key = crate::guard::cid_lifecycle::storage_key::citizen_registry(&cid);
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
            &crate::guard::cid_lifecycle::GenesisReference::default(),
        ),
        Err(crate::guard::cid_lifecycle::GuardError::CitizenAccountIdBindingMissing)
    );

    // 绑定闭环校验(增量与全量路径共用的咽喉):匿名 CID 无投票身份也放行。
    assert_eq!(
        validate_citizen_identity_binding(&cid, &|key| post.get(key).cloned()),
        Ok(())
    );

    // 同账户 + 同 CID 的 self_occupy 幂等重试会触及正反绑定但不推进
    // revision；块前/块后值完全相同，NodeGuard 必须放行。
    let idempotent_delta = BTreeMap::from([
        (
            crate::guard::cid_lifecycle::storage_key::account_id_by_cid(&cid),
            Some(account.encode()),
        ),
        (
            crate::guard::cid_lifecycle::storage_key::cid_by_account_id(&account),
            Some(cid.to_vec().encode()),
        ),
    ]);
    assert_eq!(
        check_transition(
            2,
            &idempotent_delta,
            |key| post.get(key).cloned(),
            |key| post.get(key).cloned(),
            &crate::guard::cid_lifecycle::GenesisReference::default(),
        ),
        Ok(())
    );

    // 换绑:匿名 CID 从旧账户换到新账户(admin_rebind / self_rebind 共用的绑定迁移),
    // 无投票身份也必须过守卫。
    let new_account = [10u8; 32];
    let mut rebound = anonymous_citizen_state(&cid, new_account);
    rebound.remove(&crate::guard::cid_lifecycle::storage_key::cid_by_account_id(&account));
    rebound.insert(
        crate::guard::cid_lifecycle::storage_key::binding_revision_by_cid(&cid),
        2u64.encode(),
    );
    let rebind_delta = BTreeMap::from([
        (
            crate::guard::cid_lifecycle::storage_key::account_id_by_cid(&cid),
            Some(new_account.encode()),
        ),
        (
            crate::guard::cid_lifecycle::storage_key::cid_by_account_id(&account),
            None,
        ),
        (
            crate::guard::cid_lifecycle::storage_key::cid_by_account_id(&new_account),
            Some(cid.to_vec().encode()),
        ),
        (
            crate::guard::cid_lifecycle::storage_key::binding_revision_by_cid(&cid),
            Some(2u64.encode()),
        ),
    ]);
    assert_eq!(
        check_transition(
            2,
            &rebind_delta,
            |key| post.get(key).cloned(),
            |key| rebound.get(key).cloned(),
            &crate::guard::cid_lifecycle::GenesisReference::default(),
        ),
        Ok(())
    );

    // 同块连续 A→B→C 在块级 delta 中只呈现 A→C、revision 1→3，必须放行。
    let final_account = [11u8; 32];
    let mut multi_rebound = anonymous_citizen_state(&cid, final_account);
    multi_rebound.remove(&crate::guard::cid_lifecycle::storage_key::cid_by_account_id(&account));
    multi_rebound.insert(
        crate::guard::cid_lifecycle::storage_key::binding_revision_by_cid(&cid),
        3u64.encode(),
    );
    let multi_rebind_delta = BTreeMap::from([
        (
            crate::guard::cid_lifecycle::storage_key::account_id_by_cid(&cid),
            Some(final_account.encode()),
        ),
        (
            crate::guard::cid_lifecycle::storage_key::cid_by_account_id(&account),
            None,
        ),
        (
            crate::guard::cid_lifecycle::storage_key::cid_by_account_id(&final_account),
            Some(cid.to_vec().encode()),
        ),
        (
            crate::guard::cid_lifecycle::storage_key::binding_revision_by_cid(&cid),
            Some(3u64.encode()),
        ),
    ]);
    assert_eq!(
        check_transition(
            2,
            &multi_rebind_delta,
            |key| post.get(key).cloned(),
            |key| multi_rebound.get(key).cloned(),
            &crate::guard::cid_lifecycle::GenesisReference::default(),
        ),
        Ok(())
    );

    // 同块连续 A→B→A 的最终绑定值与父状态相同，但 forward key 确实被触碰且
    // revision 1→3；不能把这种合法的两次换绑误判成 revision-only。
    let mut round_trip = post.clone();
    round_trip.insert(
        crate::guard::cid_lifecycle::storage_key::binding_revision_by_cid(&cid),
        3u64.encode(),
    );
    let round_trip_delta = BTreeMap::from([
        (
            crate::guard::cid_lifecycle::storage_key::account_id_by_cid(&cid),
            Some(account.encode()),
        ),
        (
            crate::guard::cid_lifecycle::storage_key::cid_by_account_id(&account),
            Some(cid.to_vec().encode()),
        ),
        (
            crate::guard::cid_lifecycle::storage_key::cid_by_account_id(&new_account),
            None,
        ),
        (
            crate::guard::cid_lifecycle::storage_key::binding_revision_by_cid(&cid),
            Some(3u64.encode()),
        ),
    ]);
    assert_eq!(
        check_transition(
            2,
            &round_trip_delta,
            |key| post.get(key).cloned(),
            |key| round_trip.get(key).cloned(),
            &crate::guard::cid_lifecycle::GenesisReference::default(),
        ),
        Ok(())
    );

    // 只换双向账户而不推进 revision 必须 fail-closed。
    let mut stale_revision_delta = rebind_delta.clone();
    stale_revision_delta
        .remove(&crate::guard::cid_lifecycle::storage_key::binding_revision_by_cid(&cid));
    let mut stale_revision_state = rebound.clone();
    stale_revision_state.insert(
        crate::guard::cid_lifecycle::storage_key::binding_revision_by_cid(&cid),
        1u64.encode(),
    );
    assert_eq!(
        check_transition(
            2,
            &stale_revision_delta,
            |key| post.get(key).cloned(),
            |key| stale_revision_state.get(key).cloned(),
            &crate::guard::cid_lifecycle::GenesisReference::default(),
        ),
        Err(crate::guard::cid_lifecycle::GuardError::CitizenBindingRevisionTransitionInvalid)
    );

    // 无换绑/吊销却单独推进 revision 同样拒绝。
    let revision_only = BTreeMap::from([(
        crate::guard::cid_lifecycle::storage_key::binding_revision_by_cid(&cid),
        Some(2u64.encode()),
    )]);
    let mut revision_only_state = post.clone();
    revision_only_state.insert(
        crate::guard::cid_lifecycle::storage_key::binding_revision_by_cid(&cid),
        2u64.encode(),
    );
    assert_eq!(
        check_transition(
            2,
            &revision_only,
            |key| post.get(key).cloned(),
            |key| revision_only_state.get(key).cloned(),
            &crate::guard::cid_lifecycle::GenesisReference::default(),
        ),
        Err(crate::guard::cid_lifecycle::GuardError::CitizenBindingRevisionTransitionInvalid)
    );
}

#[test]
fn candidate_identity_requires_active_normal_voter_and_locks_identity_fields() {
    let cid = citizen_cid_number("candidate-guard");
    let account = [12u8; 32];
    let parent = citizen_state(&cid, account);
    let candidate_key = crate::guard::cid_lifecycle::storage_key::candidate_identity(&cid);
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
            &crate::guard::cid_lifecycle::GenesisReference::default(),
        ),
        Ok(())
    );
    assert!(
        crate::guard::cid_lifecycle::storage_key::enumerated_prefixes()
            .contains(&crate::guard::cid_lifecycle::storage_key::candidate_identity_prefix())
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
        Err(crate::guard::cid_lifecycle::GuardError::CitizenCandidateIdentityWithoutVotingIdentity)
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
            &crate::guard::cid_lifecycle::GenesisReference::default(),
        ),
        Err(crate::guard::cid_lifecycle::GuardError::CitizenCandidateIdentityLockedFieldChanged)
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
            &crate::guard::cid_lifecycle::GenesisReference::default(),
        ),
        Ok(())
    );
    assert_eq!(
        check_transition(
            4,
            &rename_delta,
            |key| created.get(key).cloned(),
            |key| renamed.get(key).cloned(),
            &crate::guard::cid_lifecycle::GenesisReference::default(),
        ),
        Err(crate::guard::cid_lifecycle::GuardError::CitizenCandidateIdentityUpdateHeightInvalid)
    );

    // SCALE 值必须精确消费，尾随字节同样拒绝。
    let mut malformed = created.clone();
    malformed
        .get_mut(&candidate_key)
        .expect("candidate identity")
        .push(0xff);
    assert_eq!(
        validate_candidate_identity(&cid, &|key| malformed.get(key).cloned()),
        Err(
            crate::guard::cid_lifecycle::GuardError::StorageValueDecodeFailed(
                "CandidateIdentityByCid"
            )
        )
    );
}

#[test]
fn candidate_identity_can_only_be_deleted_with_cid_and_voting_revocation() {
    let cid = citizen_cid_number("candidate-revoke");
    let account = [13u8; 32];
    let candidate_key = crate::guard::cid_lifecycle::storage_key::candidate_identity(&cid);
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
            &crate::guard::cid_lifecycle::GenesisReference::default(),
        ),
        Err(crate::guard::cid_lifecycle::GuardError::CitizenCandidateIdentityDeleted)
    );

    let registry_key = crate::guard::cid_lifecycle::storage_key::citizen_registry(&cid);
    let voting_key = crate::guard::cid_lifecycle::storage_key::voting_identity(&cid);
    let revision_key = crate::guard::cid_lifecycle::storage_key::binding_revision_by_cid(&cid);
    let mut revoked = illegal;
    let mut registry: CitizenCidRecord = decode_exact(
        revoked.get(&registry_key).expect("cid registry"),
        "CidRegistry",
    )
    .expect("registry decodes");
    registry.status = crate::guard::cid_lifecycle::CitizenCidStatus::Revoked;
    registry.revoked_at = Some(3);
    revoked.insert(registry_key.clone(), registry.encode());
    let mut voting: CitizenVotingIdentity = decode_exact(
        revoked.get(&voting_key).expect("voting identity"),
        "VotingIdentityByCid",
    )
    .expect("voting identity decodes");
    voting.citizen_status = crate::guard::cid_lifecycle::CitizenStatus::Revoked;
    voting.updated_at = 3;
    revoked.insert(voting_key.clone(), voting.encode());
    revoked.insert(revision_key.clone(), 2u64.encode());
    let lawful_delta = BTreeMap::from([
        (
            registry_key,
            revoked
                .get(&crate::guard::cid_lifecycle::storage_key::citizen_registry(
                    &cid,
                ))
                .cloned(),
        ),
        (
            voting_key,
            revoked
                .get(&crate::guard::cid_lifecycle::storage_key::voting_identity(
                    &cid,
                ))
                .cloned(),
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
            &crate::guard::cid_lifecycle::GenesisReference::default(),
        ),
        Ok(())
    );
}

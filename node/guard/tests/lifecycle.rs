use super::*;

use crate::guard::cid_lifecycle::check_transition;
use crate::guard::cid_lifecycle::checksum_acc;
use crate::guard::cid_lifecycle::decode_exact;
use crate::guard::cid_lifecycle::parse_cid;
use crate::guard::cid_lifecycle::validate_candidate_identity;
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
            year: 2026,
            institution: "CTZN",
        },
    )
    .expect("valid citizen CID")
    .into_bytes()
}

fn citizen_state(cid: &[u8]) -> BTreeMap<Vec<u8>, Vec<u8>> {
    BTreeMap::from([
        (
            storage_key::citizen_registry(cid),
            CitizenCidRecord {
                registrar_cid_number: b"registrar".to_vec(),
                commitment: [3; 32],
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
            year: 2026,
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
fn candidate_identity_requires_active_normal_voter_and_locks_identity_fields() {
    let cid = citizen_cid_number("candidate-guard");
    let parent = citizen_state(&cid);
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

    // 没有投票身份时不能生成竞选身份。
    let mut without_voting = citizen_state(&cid);
    without_voting.remove(&storage_key::voting_identity(&cid));
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
    let candidate_key = crate::guard::cid_lifecycle::storage_key::candidate_identity(&cid);
    let mut parent = citizen_state(&cid);
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

#[test]
fn cid_guard_keeps_its_existing_rules_for_u32_year_cids() {
    for institution in ["CTZN", "NATP", "CGOV", "NRC"] {
        for year in [2026u32, 10000, u32::MAX] {
            let cid = primitives::cid::generator::generate_cid_number(
                primitives::cid::generator::GenerateCidNumberInput {
                    public_key: "node-cid-year",
                    p1: "1",
                    province_code: "GD",
                    province_name: "广东省",
                    city_code: "001",
                    city_name: "荔湾市",
                    year,
                    institution,
                },
            )
            .unwrap();
            assert_eq!(
                parse_cid(cid.as_bytes()).unwrap(),
                primitives::cid::code::institution_code_from_str(institution).unwrap()
            );
        }
    }
}

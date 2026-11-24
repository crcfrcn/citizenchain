//! 验证任期、来源及真实identity正反向闭环；不读运行链。
use super::*;
fn person() -> String {
    primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: "storage-person",
            p1: "1",
            province_code: "GD",
            province_name: "广东省",
            city_code: "001",
            city_name: "荔湾市",
            year: 2026,
            institution: "CTZN",
        },
    )
    .unwrap()
}
#[test]
fn source_ref_keeps_original_binary_bytes() {
    assert_eq!(display_source_ref(vec![255, 0]), "0xff00");
    assert_eq!(
        permission_operation_label(RolePermissionOperation::Propose),
        "发起提案"
    );
    assert_eq!(
        permission_operation_label(RolePermissionOperation::Vote),
        "参与投票"
    );
}
#[test]
fn effective_assignment_requires_role_status_and_inclusive_terms() {
    let mut role = RawRole {
        cid_number: b"cid".to_vec(),
        role_code: b"R".to_vec(),
        role_name: b"role".to_vec(),
        seats: 1,
        term_required: true,
        role_status: InstitutionRoleStatus::Active,
    };
    let mut a = RawAssignment {
        cid_number: b"cid".to_vec(),
        person_cid: person().into_bytes(),
        role_code: b"R".to_vec(),
        term_start: 10,
        term_end: 20,
        assignment_source: InstitutionAssignmentSource::Registry,
        assignment_source_ref: vec![],
        assignment_status: InstitutionAssignmentStatus::Active,
    };
    assert!(assignment_is_effective(&role, &a, 10));
    assert!(assignment_is_effective(&role, &a, 20));
    assert!(!assignment_is_effective(&role, &a, 9));
    assert!(!assignment_is_effective(&role, &a, 21));
    role.role_status = InstitutionRoleStatus::Inactive;
    assert!(!assignment_is_effective(&role, &a, 15));
    role.role_status = InstitutionRoleStatus::Active;
    a.assignment_status = InstitutionAssignmentStatus::Ended;
    assert!(!assignment_is_effective(&role, &a, 15));
}
fn bindings() -> BTreeMap<String, String> {
    let p = person();
    let wallet = [7u8; 32];
    let record = identity::CidRecord::<u32> {
        registrar_cid_number: b"registry".to_vec().try_into().unwrap(),
        commitment: [1; 32],
        residence_province_code: b"GD".to_vec().try_into().unwrap(),
        residence_city_code: b"001".to_vec().try_into().unwrap(),
        status: identity::CidRecordStatus::Active,
        registered_at: 1,
        revoked_at: None,
    };
    let k = |item: &str, key: Vec<u8>| nodeclient::query::map_key("Identity", item, &key);
    BTreeMap::from([
        (
            k("CidRegistry", p.as_bytes().to_vec().encode()),
            format!("0x{}", hex::encode(record.encode())),
        ),
        (
            k("AccountIdByCid", p.as_bytes().to_vec().encode()),
            format!("0x{}", hex::encode(wallet.encode())),
        ),
        (
            k("BindingRevisionByCid", p.as_bytes().to_vec().encode()),
            format!("0x{}", hex::encode(1u64.encode())),
        ),
        (
            k("CidByAccountId", wallet.encode()),
            format!("0x{}", hex::encode(p.as_bytes().to_vec().encode())),
        ),
    ])
}
#[test]
fn person_binding_requires_active_record_positive_revision_and_reverse_index() {
    let p = person();
    let mut rows = bindings();
    assert_eq!(
        person_binding(&p, &mut |k| Ok(rows.get(k).cloned())).unwrap(),
        Some((format!("0x{}", "07".repeat(32)), "1".into()))
    );
    let key = nodeclient::query::map_key(
        "Identity",
        "BindingRevisionByCid",
        &p.as_bytes().to_vec().encode(),
    );
    rows.insert(key.clone(), format!("0x{}", hex::encode(0u64.encode())));
    assert!(person_binding(&p, &mut |k| Ok(rows.get(k).cloned())).is_err());
    rows = bindings();
    rows.remove(&nodeclient::query::map_key(
        "Identity",
        "CidByAccountId",
        &[7u8; 32].encode(),
    ));
    assert!(person_binding(&p, &mut |k| Ok(rows.get(k).cloned())).is_err());
    rows.clear();
    assert_eq!(
        person_binding(&p, &mut |k| Ok(rows.get(k).cloned())).unwrap(),
        None
    );
}

fn representative_names(person: &str) -> BTreeMap<String, String> {
    use primitives::cid::china::{china_zf::CHINA_ZF, citizenchain};
    let security = CHINA_ZF
        .iter()
        .find(|entry| {
            primitives::cid::code::institution_code_from_cid_number(entry.cid_number)
                == Some(primitives::cid::code::FSC)
        })
        .unwrap();
    [
        (
            EntityPallet::Private,
            citizenchain::CITIZENCHAIN_FOUNDATION.cid_number,
        ),
        (EntityPallet::Public, security.cid_number),
    ]
    .into_iter()
    .map(|(owner, cid)| {
        let info = entitybase::InstitutionInfo {
            cid_full_name: b"institution".to_vec(),
            cid_short_name: b"institution".to_vec(),
            town_code: Vec::<u8>::new(),
            legal_representative: Some(entitybase::LegalRepresentative {
                family_name: "程".as_bytes().to_vec(),
                given_name: "伟".as_bytes().to_vec(),
                person_cid: person.as_bytes().to_vec(),
            }),
            institution_code: primitives::cid::code::institution_code_from_cid_number(cid).unwrap(),
            created_at: 0u32,
        };
        (
            storage_key(
                owner.name(),
                "Institutions",
                &[cid.as_bytes().to_vec().encode()],
            ),
            format!("0x{}", hex::encode(info.encode())),
        )
    })
    .collect()
}

#[test]
fn founder_name_uses_chain_representatives_and_rejects_conflicting_names() {
    let p = primitives::cid::china::citizenchain::LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER;
    let mut rows = representative_names(p);
    assert_eq!(
        person_name(p, &mut |k| Ok(rows.get(k).cloned())).unwrap(),
        ("程".into(), "伟".into())
    );
    let key = rows.keys().next().unwrap().clone();
    let mut info: entitybase::InstitutionInfo<u32, Vec<u8>, Vec<u8>> =
        decode_exact(&rows[&key], "Institutions").unwrap();
    info.legal_representative.as_mut().unwrap().given_name = "其他".as_bytes().to_vec();
    rows.insert(key, format!("0x{}", hex::encode(info.encode())));
    assert!(person_name(p, &mut |k| Ok(rows.get(k).cloned())).is_err());
    assert!(person_name(p, &mut |_| Err("同块读取失败".into())).is_err());
}

#[test]
fn founder_name_never_invents_identity_and_prefers_existing_person_profile() {
    let p = primitives::cid::china::citizenchain::LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER;
    let mut rows = representative_names(&person());
    assert_eq!(
        person_name(p, &mut |k| Ok(rows.get(k).cloned())).unwrap(),
        (String::new(), String::new())
    );
    assert_eq!(
        person_name(&person(), &mut |k| Ok(rows.get(k).cloned())).unwrap(),
        (String::new(), String::new())
    );
    let profile = identity::CandidateIdentity {
        birth_province_code: Default::default(),
        birth_city_code: Default::default(),
        birth_town_code: Default::default(),
        family_name: "档".as_bytes().to_vec().try_into().unwrap(),
        given_name: "案".as_bytes().to_vec().try_into().unwrap(),
        citizen_sex: identity::CitizenSex::Male,
        birth_date: 19880520,
        updated_at: 725u32,
    };
    let key = nodeclient::query::map_key(
        "Identity",
        "CandidateIdentityByCid",
        &p.as_bytes().to_vec().encode(),
    );
    rows.insert(key.clone(), format!("0x{}", hex::encode(profile.encode())));
    let mut seen = Vec::new();
    assert_eq!(
        person_name(p, &mut |k| {
            seen.push(k.to_string());
            Ok(rows.get(k).cloned())
        })
        .unwrap(),
        ("档".into(), "案".into())
    );
    assert_eq!(seen, vec![key]);
}

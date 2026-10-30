use crate::codec::decode_institution_admins;
use adminbase::Admin;
use adminbase::InstitutionAdmins;
use codec::Decode;

fn fixture_case(name: &str) -> Vec<u8> {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtime/primitives/tests/fixtures/role_permission.json"
    )))
    .expect("岗位权限 fixture 必须是合法 JSON");
    let encoded = fixture["cases"]
        .as_array()
        .and_then(|cases| {
            cases
                .iter()
                .find(|case| case["name"].as_str() == Some(name))
        })
        .and_then(|case| case["encoded_hex"].as_str())
        .expect("岗位权限 fixture 用例必须存在");
    hex::decode(encoded).expect("岗位权限 fixture 必须是合法 hex")
}

fn decode_exact<T: Decode>(bytes: &[u8]) -> T {
    let mut input = bytes;
    let decoded = T::decode(&mut input).expect("fixture SCALE 必须可解码");
    assert!(input.is_empty(), "fixture SCALE 不得存在尾随字节");
    decoded
}

#[test]
fn public_institution_admins_decode_citizen_identity_fields() {
    use codec::Encode;
    let bytes = InstitutionAdmins {
        institution_code: *b"NRC\0",
        admins: vec![
            Admin {
                account_id: [0xaau8; 32],
                cid_number: adminbase::AdminCidNumber::truncate_from(
                    b"CN220-CTZN2-198805200-2026".to_vec(),
                ),
                family_name: adminbase::FamilyName::truncate_from("张".as_bytes().to_vec()),
                given_name: adminbase::GivenName::truncate_from("三".as_bytes().to_vec()),
            },
            Admin {
                account_id: [0xbbu8; 32],
                cid_number: Default::default(),
                family_name: Default::default(),
                given_name: Default::default(),
            },
        ],
    }
    .encode();
    let decoded = decode_institution_admins(&bytes, true).unwrap();
    assert_eq!(decoded.institution_code, *b"NRC\0");
    assert_eq!(
        decoded.admins[0].account_id,
        format!("0x{}", "aa".repeat(32))
    );
    assert_eq!(decoded.admins[0].cid_number, "CN220-CTZN2-198805200-2026");
    assert_eq!(decoded.admins[0].family_name, "张");
    assert_eq!(decoded.admins[0].given_name, "三");
    assert!(decoded.admins[1].cid_number.is_empty());
    assert!(decoded.admins[1].family_name.is_empty());
    assert!(decoded.admins[1].given_name.is_empty());
}

#[test]
fn account_only_layout_and_empty_person_name_are_rejected() {
    use codec::Encode;

    let old_layout = (*b"NRC\0", vec![[0xaau8; 32]]).encode();
    assert!(decode_institution_admins(&old_layout, true).is_err());

    let empty_name = InstitutionAdmins {
        institution_code: *b"NRC\0",
        admins: vec![Admin {
            account_id: [0xaau8; 32],
            cid_number: Default::default(),
            family_name: adminbase::FamilyName::truncate_from(Vec::new()),
            given_name: adminbase::GivenName::truncate_from("员".as_bytes().to_vec()),
        }],
    }
    .encode();
    assert!(decode_institution_admins(&empty_name, false).is_err());
}

#[test]
fn private_institution_admins_preserve_citizen_cid() {
    use codec::Encode;

    let bytes = InstitutionAdmins {
        institution_code: *b"SFGY",
        admins: vec![Admin {
            account_id: [0x24u8; 32],
            cid_number: adminbase::AdminCidNumber::truncate_from(
                b"CN220-CTZN2-198805200-2026".to_vec(),
            ),
            family_name: adminbase::FamilyName::truncate_from("程".as_bytes().to_vec()),
            given_name: adminbase::GivenName::truncate_from("伟".as_bytes().to_vec()),
        }],
    }
    .encode();

    let decoded = decode_institution_admins(&bytes, false).unwrap();
    assert_eq!(decoded.institution_code, *b"SFGY");
    assert_eq!(decoded.admins[0].cid_number, "CN220-CTZN2-198805200-2026");
    assert_eq!(decoded.admins[0].family_name, "程");
    assert_eq!(decoded.admins[0].given_name, "伟");
}

#[test]
fn institution_role_permission_fixture_matches_shared_rust_types() {
    use entitybase::{AuthorizationSubject, BusinessActionId, RoleBusinessPermission, RoleSubject};
    use frame_support::pallet_prelude::{ConstU32, DecodeWithMemTracking};
    use votebase::VotePlan;

    let role: RoleSubject<Vec<u8>, Vec<u8>> =
        decode_exact(&fixture_case("role_subject_nrc_committee"));
    assert_eq!(role.cid_number, b"LN001-NRC0G-944805165-2026");
    assert_eq!(role.role_code, b"COMMITTEE_MEMBER");

    let action: BusinessActionId<Vec<u8>> =
        decode_exact(&fixture_case("business_action_resolution_issuance"));
    assert_eq!(action.module_tag, b"res-iss");
    assert_eq!(action.action_code, 0);

    let _: RoleBusinessPermission<Vec<u8>, Vec<u8>, Vec<u8>> =
        decode_exact(&fixture_case("permission_resolution_issuance_propose"));
    let _: AuthorizationSubject<Vec<u8>, Vec<u8>, [u8; 32]> =
        decode_exact(&fixture_case("authorization_personal_multisig"));

    type FixturePlan = VotePlan<[u8; 32], ConstU32<32>>;
    let plan: FixturePlan = decode_exact(&fixture_case("vote_plan_resolution_issuance_joint"));
    assert_eq!(plan.voter_subjects.len(), 3);
    assert_eq!(plan.business_object_hash, [0xabu8; 32]);

    fn requires_mem_tracking<T: DecodeWithMemTracking>() {}
    requires_mem_tracking::<FixturePlan>();
}

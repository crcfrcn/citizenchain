use admin_primitives::{Admin, InstitutionAdmins};
use codec::Decode;

use super::types::{AdminDecoded, InstitutionAdminsDecoded};

/// 解码机构管理员模块的 `InstitutionAdmins`。
///
/// 公权、私权机构统一采用 `account_id + cid_number + family_name + given_name`。
/// 机构 CID 只存在于 storage key；记录内的 `cid_number` 是管理员公民 CID 引用。
/// 岗位、任期和来源另查 entity。
pub fn decode_institution_admins(
    data: &[u8],
    is_public: bool,
) -> Result<InstitutionAdminsDecoded, String> {
    if is_public {
        return decode_public_institution_admins(data);
    }
    decode_private_institution_admins(data)
}

fn decode_private_institution_admins(data: &[u8]) -> Result<InstitutionAdminsDecoded, String> {
    type RawInstitutionAdmins = InstitutionAdmins<Vec<Admin<[u8; 32]>>>;
    let mut input = data;
    let decoded = RawInstitutionAdmins::decode(&mut input)
        .map_err(|e| format!("InstitutionAdmins SCALE 解码失败: {e}"))?;
    if !input.is_empty() {
        return Err("InstitutionAdmins 存在尾随字节".to_string());
    }
    let admins = decoded
        .admins
        .into_iter()
        .map(|admin| {
            let cid_number = String::from_utf8(admin.cid_number.into_inner())
                .map_err(|_| "管理员 cid_number 不是 UTF-8".to_string())?;
            let family_name = String::from_utf8(admin.family_name.into_inner())
                .map_err(|_| "管理员 family_name 不是 UTF-8".to_string())?;
            let given_name = String::from_utf8(admin.given_name.into_inner())
                .map_err(|_| "管理员 given_name 不是 UTF-8".to_string())?;
            if family_name.is_empty() || given_name.is_empty() {
                return Err("管理员 family_name/given_name 不得为空".to_string());
            }
            Ok(AdminDecoded {
                account_id: format!("0x{}", hex::encode(admin.account_id)),
                cid_number,
                family_name,
                given_name,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(InstitutionAdminsDecoded {
        institution_code: decoded.institution_code,
        admins,
    })
}

fn decode_public_institution_admins(data: &[u8]) -> Result<InstitutionAdminsDecoded, String> {
    type RawInstitutionAdmins = InstitutionAdmins<Vec<Admin<[u8; 32]>>>;
    let mut input = data;
    let decoded = RawInstitutionAdmins::decode(&mut input)
        .map_err(|e| format!("PublicInstitutionAdmins SCALE 解码失败: {e}"))?;
    if !input.is_empty() {
        return Err("PublicInstitutionAdmins 存在尾随字节".to_string());
    }
    let admins = decoded
        .admins
        .into_iter()
        .map(|admin| {
            Ok(AdminDecoded {
                account_id: format!("0x{}", hex::encode(admin.account_id)),
                cid_number: String::from_utf8(admin.cid_number.into_inner())
                    .map_err(|_| "公权管理员 cid_number 不是 UTF-8".to_string())?,
                family_name: String::from_utf8(admin.family_name.into_inner())
                    .map_err(|_| "公权管理员 family_name 不是 UTF-8".to_string())?,
                given_name: String::from_utf8(admin.given_name.into_inner())
                    .map_err(|_| "公权管理员 given_name 不是 UTF-8".to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(InstitutionAdminsDecoded {
        institution_code: decoded.institution_code,
        admins,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_case(name: &str) -> Vec<u8> {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../citizenchain/runtime/tests/fixtures/role_permission.json"
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
                    cid_number: admin_primitives::AdminCidNumber::truncate_from(
                        b"CN220-CTZN2-198805200-2026".to_vec(),
                    ),
                    family_name: admin_primitives::FamilyName::truncate_from(
                        "张".as_bytes().to_vec(),
                    ),
                    given_name: admin_primitives::GivenName::truncate_from(
                        "三".as_bytes().to_vec(),
                    ),
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
                family_name: admin_primitives::FamilyName::truncate_from(Vec::new()),
                given_name: admin_primitives::GivenName::truncate_from("员".as_bytes().to_vec()),
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
                cid_number: admin_primitives::AdminCidNumber::truncate_from(
                    b"CN220-CTZN2-198805200-2026".to_vec(),
                ),
                family_name: admin_primitives::FamilyName::truncate_from("程".as_bytes().to_vec()),
                given_name: admin_primitives::GivenName::truncate_from("伟".as_bytes().to_vec()),
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
        use entity_primitives::{
            AuthorizationSubject, BusinessActionId, RoleBusinessPermission, RoleSubject,
        };
        use frame_support::pallet_prelude::{ConstU32, DecodeWithMemTracking};
        use votingengine::VotePlan;

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
}

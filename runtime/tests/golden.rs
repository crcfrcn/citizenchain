//! 从现行Runtime和共享类型编码跨端金标，保留历史拒绝样本及全部调用场景。

use super::*;
use codec::{DecodeAll, Encode};
use entitybase::{
    InstitutionAssignmentSource, InstitutionAssignmentStatus, InstitutionAssignmentTarget,
    InstitutionGovernanceAction, InstitutionPolicyAction, InstitutionRoleMutation,
    InstitutionRoleRegistration, InstitutionRoleStatus, RolePermissionChange, RolePermissionSpec,
};
use serde_json::{json, Value};

const CID: &str = "LN001-NRC0G-944805165-2026";
const PEOPLE: [&str; 2] = ["CN220-CTZN2-198805200-2026", "CN951-CTZN1-539598435-2026"];

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("primitives/tests/fixtures")
        .join(name)
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn permission(module: &[u8]) -> RolePermissionSpec {
    RolePermissionSpec {
        business_action_id: entitybase::BusinessActionId {
            module_tag: module.to_vec(),
            action_code: 0,
        },
        operation: entitybase::RolePermissionOperation::Propose,
    }
}

fn role_mutation(registration: bool) -> InstitutionRoleMutation {
    InstitutionRoleMutation::Create {
        role_name: "财务岗".as_bytes().to_vec(),
        term_required: false,
        seats: 2,
        permissions: vec![permission(b"res-iss")],
        assignments: PEOPLE
            .iter()
            .map(|person| InstitutionAssignmentTarget {
                person_cid: person.as_bytes().to_vec(),
                term_start: 0,
                term_end: 0,
                assignment_source: if registration {
                    InstitutionAssignmentSource::Registry
                } else {
                    InstitutionAssignmentSource::InstitutionGovernance
                },
                assignment_source_ref: vec![],
                assignment_status: InstitutionAssignmentStatus::Active,
            })
            .collect(),
    }
}

fn institution_call(name: &str) -> RuntimeCall {
    let private = name.contains("private");
    let base = if private {
        "LN001-CCOR0-944805165-2026"
    } else {
        "LN001-CGOV0-944805165-2026"
    };
    let cid =
        if name.starts_with("configure_") || name.contains("_policy") || name.starts_with("add_") {
            base
        } else {
            CID
        };
    let action = InstitutionGovernanceAction {
        role_mutations: vec![if name.starts_with("configure_") {
            InstitutionRoleMutation::Configure {
                role_code: b"R_POLICY".to_vec(),
                term_required: true,
                seats: 2,
                role_status: InstitutionRoleStatus::Active,
            }
        } else {
            role_mutation(false)
        }],
        assignment_changes: vec![],
        legal_representative_change: None,
    };
    let policy = InstitutionPolicyAction {
        permission_changes: vec![RolePermissionChange {
            role_code: b"R_POLICY".to_vec(),
            permissions: vec![permission(b"multisig")],
        }],
        threshold: Some(2),
    };
    let registration = InstitutionRoleRegistration {
        role_mutations: vec![role_mutation(true)],
        assignment_changes: vec![],
    };
    macro_rules! build {
        ($module:ident, $variant:ident) => {{
            let call = if name.starts_with("register_") {
                $module::Call::<Runtime>::register_institution_roles {
                    cid_number: cid.as_bytes().to_vec().try_into().unwrap(),
                    action: registration,
                    actor_cid_number: b"ZS001-FRG07-249474503-2026".to_vec(),
                    actor_role_code: b"RCHAIR".to_vec(),
                    expected_state_hash: [0xab; 32],
                    initial_threshold: Some(2),
                }
            } else if name.contains("_policy") {
                $module::Call::<Runtime>::propose_institution_policy {
                    cid_number: cid.as_bytes().to_vec().try_into().unwrap(),
                    action: policy,
                    proposer_role_code: b"R_GOV".to_vec().try_into().unwrap(),
                    expected_state_hash: [9; 32],
                }
            } else if name.starts_with("add_") {
                $module::Call::<Runtime>::propose_add_institution_account {
                    cid_number: cid.as_bytes().to_vec().try_into().unwrap(),
                    account_names: vec!["业务账户".as_bytes().to_vec().try_into().unwrap()]
                        .try_into()
                        .unwrap(),
                    proposer_role_code: b"R_GOV".to_vec().try_into().unwrap(),
                }
            } else {
                $module::Call::<Runtime>::propose_institution_governance {
                    cid_number: cid.as_bytes().to_vec().try_into().unwrap(),
                    action,
                    actor_cid_number: cid.as_bytes().to_vec(),
                    proposer_role_code: if name.starts_with("configure_") {
                        b"R_GOV".to_vec()
                    } else {
                        b"RCHAIR".to_vec()
                    }
                    .try_into()
                    .unwrap(),
                }
            };
            RuntimeCall::$variant(call)
        }};
    }
    if private {
        build!(privateentity, PrivateEntity)
    } else {
        build!(publicentity, PublicEntity)
    }
}

fn call_for(name: &str) -> RuntimeCall {
    match name {
        "cast_referendum" => RuntimeCall::JointVote(jointvote::Call::cast_referendum {
            proposal_id: 99,
            approve: true,
        }),
        "propose_issuance" => {
            RuntimeCall::ResolutionIssue(resolutionissue::Call::propose_issuance {
                actor_cid_number: CID.as_bytes().to_vec().try_into().unwrap(),
                proposer_role_code: b"COMMITTEE_MEMBER".to_vec().try_into().unwrap(),
                reason: "紧急救灾".as_bytes().to_vec().try_into().unwrap(),
                total_amount: 50_000_000,
                allocations: [0xa1, 0xa2]
                    .into_iter()
                    .map(|byte| resolutionissue::proposal::RecipientAmount {
                        recipient_account_id: AccountId::from([byte; 32]),
                        amount: 0,
                    })
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap(),
            })
        }
        "batch" | "batch_all" | "force_batch" => {
            let calls = (0..8)
                .map(|_| {
                    RuntimeCall::Onchain(onchain::pallet::Call::transfer_with_remark {
                        beneficiary_account_id: AccountId::from([0xaa; 32]),
                        amount: 10_500,
                        remark: "转账".as_bytes().to_vec().try_into().unwrap(),
                    })
                })
                .collect();
            RuntimeCall::Utility(match name {
                "batch" => pallet_utility::Call::batch { calls },
                "batch_all" => pallet_utility::Call::batch_all { calls },
                _ => pallet_utility::Call::force_batch { calls },
            })
        }
        "propose_public_institution_governance"
        | "register_public_institution_roles"
        | "propose_private_institution_governance"
        | "register_private_institution_roles"
        | "propose_public_institution_policy"
        | "configure_public_institution_role"
        | "add_public_institution_account"
        | "propose_private_institution_policy"
        | "configure_private_institution_role"
        | "add_private_institution_account" => institution_call(name),
        _ => panic!("未登记的调用金标场景：{name}"),
    }
}

fn encode_calls(value: &mut Value) {
    assert_eq!(value["cases"].as_array().unwrap().len(), 9);
    assert_eq!(value["institution_extensions"].as_array().unwrap().len(), 6);
    let mut names = std::collections::BTreeSet::new();
    for group in ["cases", "institution_extensions"] {
        for row in value[group].as_array_mut().unwrap() {
            assert!(names.insert(row["name"].as_str().unwrap().to_owned()));
            let bytes = call_for(row["name"].as_str().unwrap()).encode();
            assert_eq!(u64::from(bytes[0]), row["pallet_index"].as_u64().unwrap());
            assert_eq!(u64::from(bytes[1]), row["call_index"].as_u64().unwrap());
            assert_eq!(
                RuntimeCall::decode_all(&mut bytes.as_slice())
                    .unwrap()
                    .encode(),
                bytes
            );
            row["expected_call_data_hex"] = json!(hex(&bytes));
            row["expected_byte_length"] = json!(bytes.len());
        }
    }
    for row in value["rejected_calls"].as_array().unwrap() {
        let raw = row["expected_call_data_hex"]
            .as_str()
            .unwrap()
            .trim_start_matches("0x");
        assert!(
            RuntimeCall::decode_all(&mut hex::decode(raw).unwrap().as_slice()).is_err(),
            "retired call layout accepted"
        );
    }
}

fn encode_permissions(value: &mut Value) {
    use engingcore::types::{
        AuthorizationSubject, BusinessActionId, CidNumber, InstitutionVoteTicket, RoleCode,
        RoleSubject, VotePlanOf, VotingEngineKind,
    };
    let subject = |cid: &str, role: &[u8]| RoleSubject::<CidNumber, RoleCode> {
        cid_number: cid.as_bytes().to_vec().try_into().unwrap(),
        role_code: role.to_vec().try_into().unwrap(),
    };
    let role = subject(CID, b"COMMITTEE_MEMBER");
    let action = BusinessActionId {
        module_tag: b"res-iss".to_vec().try_into().unwrap(),
        action_code: 0,
    };
    let permission = entitybase::RoleBusinessPermission {
        role_subject: role.clone(),
        business_action_id: action.clone(),
        operation: entitybase::RolePermissionOperation::Propose,
    };
    let personal = AuthorizationSubject::<CidNumber, RoleCode, AccountId>::PersonalMultisig(
        AccountId::from([7; 32]),
    );
    let plan = VotePlanOf::<AccountId> {
        business_action_id: action.clone(),
        proposal_owner: b"res-iss".to_vec().try_into().unwrap(),
        proposer_subject: AuthorizationSubject::Institution(role.clone()),
        voter_subjects: vec![
            AuthorizationSubject::Institution(role.clone()),
            AuthorizationSubject::Institution(subject(
                "ZS001-PRC0E-016974075-2026",
                b"COMMITTEE_MEMBER",
            )),
            AuthorizationSubject::Institution(subject("ZS001-PRB08-233384677-2026", b"DIRECTOR")),
        ]
        .try_into()
        .unwrap(),
        voting_engine: VotingEngineKind::Joint,
        business_object_hash: [0xab; 32],
    };
    let bytes = [
        role.encode(),
        action.encode(),
        permission.encode(),
        personal.encode(),
        plan.encode(),
    ];
    assert_eq!(value["cases"].as_array().unwrap().len(), bytes.len());
    for (row, encoded) in value["cases"].as_array_mut().unwrap().iter_mut().zip(bytes) {
        row["encoded_hex"] = json!(hex::encode(encoded));
    }
    let voters: frame_support::BoundedVec<
        CidNumber,
        <Runtime as engingcore::Config>::MaxSnapshotVoters,
    > = PEOPLE
        .iter()
        .map(|person| person.as_bytes().to_vec().try_into().unwrap())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let ticket = InstitutionVoteTicket {
        role_subject: role,
        person_cid: voters[0].clone(),
    };
    value["role_voters"] = json!({"cid_number":CID,"role_code":"COMMITTEE_MEMBER", "person_cids":PEOPLE,
        "voters_hex":hex(voters.encode()), "original_ticket_hex":hex(ticket.encode())});
    for key in ["self_call_hex", "registry_call_hex"] {
        let raw = value["rejected_legacy"][key]
            .as_str()
            .unwrap()
            .trim_start_matches("0x");
        let bytes = hex::decode(raw).unwrap();
        assert!(
            RuntimeCall::decode_all(&mut bytes.as_slice()).is_err(),
            "retired call accepted: {key}"
        );
    }
    let raw = value["rejected_legacy"]["governance_action_hex"]
        .as_str()
        .unwrap()
        .trim_start_matches("0x");
    assert!(
        InstitutionGovernanceAction::decode_all(&mut hex::decode(raw).unwrap().as_slice()).is_err()
    );
}

fn encode_binary(value: &mut Value) {
    use primitives::sign::{binary_domain_prefix, clearing_decrypt_payload, role_access_payload};
    assert_eq!(value["domain"].as_str(), Some("GMB"));
    let tags: std::collections::BTreeSet<_> = value["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["op_tag"].as_str().unwrap())
        .collect();
    assert_eq!(value["vectors"].as_array().unwrap().len(), 2);
    assert_eq!(tags, ["0x18", "0x19"].into_iter().collect());
    for row in value["vectors"].as_array_mut().unwrap() {
        let input = &row["sample_inputs"];
        let cid = input["cid_number"].as_str().unwrap().as_bytes();
        let public: [u8; 32] = hex::decode(input["pubkey_hex"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        let nonce: [u8; 16] = hex::decode(input["nonce_hex"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        let timestamp = input["timestamp"].as_u64().unwrap();
        let tag = u8::from_str_radix(row["op_tag"].as_str().unwrap().trim_start_matches("0x"), 16)
            .unwrap();
        let payload = match tag {
            0x18 => {
                let code: [u8; 4] = hex::decode(input["institution_code_hex"].as_str().unwrap())
                    .unwrap()
                    .try_into()
                    .unwrap();
                role_access_payload(
                    cid,
                    &code,
                    input["kind"].as_u64().unwrap() as u8,
                    &public,
                    timestamp,
                    &nonce,
                )
                .unwrap()
            }
            0x19 => clearing_decrypt_payload(cid, &public, timestamp, &nonce).unwrap(),
            _ => panic!("unregistered binary fixture"),
        };
        row["prefix_hex"] = json!(hex::encode(binary_domain_prefix(tag)));
        row["total_len"] = json!(payload.len());
        row["payload_hex"] = json!(hex::encode(payload));
    }
}

#[test]
fn runtime_fixture_contracts() {
    let mut names: Vec<_> = std::fs::read_dir(fixture_path(""))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "account.json",
            "binary.json",
            "call.json",
            "domain.json",
            "permission.json",
            "scale.json"
        ]
    );
    for (name, generate) in [
        ("call.json", encode_calls as fn(&mut Value)),
        ("permission.json", encode_permissions),
        ("binary.json", encode_binary),
    ] {
        let path = fixture_path(name);
        let before: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let mut actual = before.clone();
        generate(&mut actual);
        if std::env::var("RUNTIME_GOLDEN_UPDATE").as_deref() == Ok("1") {
            std::fs::write(&path, serde_json::to_string_pretty(&actual).unwrap() + "\n").unwrap();
        } else {
            assert_eq!(
                actual, before,
                "canonical fixture differs from actual Runtime types: {name}"
            );
        }
    }
}

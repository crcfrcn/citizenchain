//! 原所属模块回归，断言与输入保持。
use super::*;

fn private_admin(
    account_id: [u8; 32],
    family_name: &str,
    given_name: &str,
) -> adminbase::Admin<[u8; 32]> {
    adminbase::Admin {
        account_id,
        // Phase 1: 私权/个人多签管理员公民 CID 预留空。
        cid_number: Default::default(),
        family_name: family_name
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("family name fits"),
        given_name: given_name
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("given name fits"),
    }
}

fn public_admin(
    account_id: [u8; 32],
    cid_number: &str,
    family_name: &str,
    given_name: &str,
) -> adminbase::Admin<[u8; 32]> {
    adminbase::Admin {
        account_id,
        cid_number: cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("citizen cid fits"),
        family_name: family_name
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("family name fits"),
        given_name: given_name
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("given name fits"),
    }
}

#[test]
fn admin_payload_encodes_account_cid_family_name_and_given_name() {
    let admins = vec![
        private_admin([1; 32], "张", "三"),
        private_admin([2; 32], "管理", "员"),
    ];
    let expected = admins
        .iter()
        .map(|admin| {
            (
                admin.account_id,
                admin.cid_number.clone(),
                admin.family_name.clone(),
                admin.given_name.clone(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        encode_admins_payload(&InstitutionAdminsPayload::Private(admins)),
        expected.encode()
    );
}

#[test]
fn public_admin_payload_encodes_account_cid_family_name_and_given_name() {
    let admins = vec![public_admin([3; 32], "CN220-CTZN2-198805200-2026", "", "")];
    let expected = admins
        .iter()
        .map(|admin| {
            (
                admin.account_id,
                admin.cid_number.clone(),
                admin.family_name.clone(),
                admin.given_name.clone(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        encode_admins_payload(&InstitutionAdminsPayload::Public(admins)),
        expected.encode()
    );
}

#[test]
fn governance_payload_matches_runtime_call_field_order() {
    let args = ProposeInstitutionGovernanceArgs {
        cid_number: b"LN001-SFAS-0001".to_vec(),
        governance_action: vec![0, 8, b'A'],
        institution_code: *b"SFAS",
        actor_cid_number: b"LN001-SFAS-0001".to_vec(),
        proposer_role_code: b"RFINANCE".to_vec(),
    };
    let encoded = encode_propose_institution_governance(&args);
    assert_eq!(&encoded.call_data[..2], &[31, 8]);
    assert_eq!(encoded.action, 0x1f08);
}

#[test]
fn register_admins_payload_matches_runtime_call_field_order() {
    use codec::DecodeAll;
    // 公私权均解码真实RuntimeCall，含初始化/后续登记；旧格式及任意尾随字节拒绝。
    for (institution_code, pallet) in [(*b"CFIN", 30u8), (*b"SFAS", 31u8)] {
        let admins = vec![
            private_admin([1; 32], "张", "三"),
            private_admin([2; 32], "李", "四"),
        ];
        for threshold in [None, Some(2u32)] {
            let action = if threshold.is_some() {
                entitybase::InstitutionGovernanceAction::ReplaceAdminsAndMutateRoles {
                    admins: admins.clone(),
                    role_mutations: vec![entitybase::InstitutionRoleMutation::Create {
                        role_name: "办理岗位".as_bytes().to_vec(),
                        term_required: false,
                        permissions: vec![entitybase::RolePermissionSpec {
                            business_action_id: entitybase::BusinessActionId {
                                module_tag: b"ins-reg".to_vec(),
                                action_code: 0,
                            },
                            operation: entitybase::RolePermissionOperation::Propose,
                        }],
                        assignments: vec![entitybase::InstitutionAssignmentTarget {
                            account_id: [1; 32],
                            term_start: 0,
                            term_end: 0,
                            assignment_source: entitybase::InstitutionAssignmentSource::Registry,
                            assignment_source_ref: vec![],
                            assignment_status: entitybase::InstitutionAssignmentStatus::Active,
                        }],
                    }],
                    assignment_changes: vec![],
                    legal_representative_change: None,
                }
            } else {
                entitybase::InstitutionGovernanceAction::ReplaceAdmins {
                    admins: admins.clone(),
                }
            };
            let args = RegisterInstitutionAdminsArgs {
                cid_number: b"LN001-SFAS1-123456789-2026".to_vec(),
                action,
                institution_code,
                actor_cid_number: b"LN001-FRG00-000000001-2026".to_vec(),
                actor_role_code: b"REGISTRY-ROLE".to_vec(),
                expected_state_hash: [0xab; 32],
                initial_threshold: threshold,
            };
            let encoded = encode_register_institution_admins(&args);
            let mut expected = vec![pallet, 9];
            expected.extend(
                (
                    &args.cid_number,
                    &args.action,
                    &args.actor_cid_number,
                    &args.actor_role_code,
                    args.expected_state_hash,
                    args.initial_threshold,
                )
                    .encode(),
            );
            assert_eq!(encoded.call_data, expected);
            assert_eq!(encoded.action, u16::from(pallet) << 8 | 9);
            let decoded = runtime::RuntimeCall::decode_all(&mut &encoded.call_data[..]).unwrap();
            assert_eq!(decoded.encode(), encoded.call_data);
            // 每个新增字段的截断/非法Option和旧名册格式都不能误解码。
            for cut in [1, 2, 32, 33] {
                let end = encoded.call_data.len() - cut;
                assert!(runtime::RuntimeCall::decode_all(&mut &encoded.call_data[..end]).is_err());
            }
            let mut trailing = encoded.call_data.clone();
            trailing.push(0);
            assert!(runtime::RuntimeCall::decode_all(&mut &trailing[..]).is_err());
            let mut legacy = vec![pallet, 9];
            legacy.extend(
                (
                    &args.cid_number,
                    &admins,
                    &args.actor_cid_number,
                    &args.actor_role_code,
                )
                    .encode(),
            );
            assert!(runtime::RuntimeCall::decode_all(&mut &legacy[..]).is_err());
        }
    }
}

#[test]
fn add_account_payload_matches_runtime_call_field_order() {
    // CFIN(市财政)= 公权码 → PublicManage(30)。
    let args = ProposeAddInstitutionAccountArgs {
        cid_number: b"LN001-CFIN-0001".to_vec(),
        account_names: vec![
            "专项账户".as_bytes().to_vec(),
            "结算账户".as_bytes().to_vec(),
        ],
        institution_code: *b"CFIN",
        proposer_role_code: b"RFINANCE".to_vec(),
    };
    let encoded = encode_propose_add_institution_account(&args);
    assert_eq!(&encoded.call_data[..2], &[30, 7]);
    assert_eq!(encoded.action, 0x1e07);
    // 逐字节金标:pallet/call ++ SCALE(cid) ++ SCALE(Vec<Vec<u8>>) ++ SCALE(role)。
    let mut expected = vec![30u8, 7u8];
    expected.extend(args.cid_number.encode());
    expected.extend(args.account_names.encode());
    expected.extend(args.proposer_role_code.encode());
    assert_eq!(encoded.call_data, expected);
}

#[test]
fn close_institution_payload_matches_runtime_call_field_order() {
    // CFIN(市财政)= 公权码 → PublicManage(30)。
    let args = ProposeCloseInstitutionArgs {
        actor_cid_number: b"LN001-CFIN-0001".to_vec(),
        proposer_role_code: b"RFINANCE".to_vec(),
        institution_account_id: [7u8; 32],
        beneficiary_account_id: [9u8; 32],
        institution_code: *b"CFIN",
    };
    let encoded = encode_propose_close_institution(&args);
    assert_eq!(&encoded.call_data[..2], &[30, 1]);
    assert_eq!(encoded.action, 0x1e01);
    // 逐字节金标:pallet/call ++ SCALE(actor_cid) ++ SCALE(role) ++ 32B ++ 32B(AccountId 无长度前缀)。
    let mut expected = vec![30u8, 1u8];
    expected.extend(args.actor_cid_number.encode());
    expected.extend(args.proposer_role_code.encode());
    expected.extend_from_slice(&args.institution_account_id);
    expected.extend_from_slice(&args.beneficiary_account_id);
    assert_eq!(encoded.call_data, expected);
}

#[test]
fn private_institution_routes_add_and_close_to_pallet_31() {
    // SFAS = 私权码(见 primitives::cid::code::is_private_legal_code)→ PrivateManage(31)。
    let add = encode_propose_add_institution_account(&ProposeAddInstitutionAccountArgs {
        cid_number: b"LN001-SFAS-0001".to_vec(),
        account_names: vec![b"payroll".to_vec()],
        institution_code: *b"SFAS",
        proposer_role_code: b"ROWNER".to_vec(),
    });
    assert_eq!(&add.call_data[..2], &[31, 7]);
    assert_eq!(add.action, 0x1f07);
    let close = encode_propose_close_institution(&ProposeCloseInstitutionArgs {
        actor_cid_number: b"LN001-SFAS-0001".to_vec(),
        proposer_role_code: b"ROWNER".to_vec(),
        institution_account_id: [1u8; 32],
        beneficiary_account_id: [2u8; 32],
        institution_code: *b"SFAS",
    });
    assert_eq!(&close.call_data[..2], &[31, 1]);
    assert_eq!(close.action, 0x1f01);
}

#[test]
fn registration_call_golden_is_shared_with_cold_wallet() {
    use codec::DecodeAll;
    let args = RegisterInstitutionAdminsArgs {
        cid_number: b"NX013-CREGN-937676656-2026".to_vec(),
        action: entitybase::InstitutionGovernanceAction::ReplaceAdmins {
            admins: vec![private_admin([0x41; 32], "", "")],
        },
        institution_code: *b"CREG",
        actor_cid_number: b"ZS001-FRG07-249474503-2026".to_vec(),
        actor_role_code: b"RCHAIR".to_vec(),
        expected_state_hash: [0xab; 32],
        initial_threshold: None,
    };
    let encoded = encode_register_institution_admins(&args).call_data;
    assert_eq!(hex::encode(&encoded), "1e09684e583031332d435245474e2d3933373637363635362d3230323600044141414141414141414141414141414141414141414141414141414141414141000000685a533030312d46524730372d3234393437343530332d3230323618524348414952abababababababababababababababababababababababababababababababab00");
    assert_eq!(
        runtime::RuntimeCall::decode_all(&mut &encoded[..])
            .unwrap()
            .encode(),
        encoded
    );
}

//! 验证私权机构账户、岗位和治理登记的授权及状态变化。

use super::*;
use engingcore::{types::code_bytes, InternalVoteEngine as _};
use frame_support::traits::GetCallName;
use frame_support::{assert_noop, assert_ok};
use sp_core::{sr25519, Pair};

#[test]
fn direct_institution_creation_call_is_permanently_absent() {
    let calls = <pallet::Call<Test> as GetCallName>::get_call_names();
    assert!(!calls.contains(&"propose_create_private_institution"));
}

fn governance_assignment(
    account_id: AccountId32,
) -> entitybase::InstitutionAssignmentTarget<AccountId32> {
    entitybase::InstitutionAssignmentTarget {
        account_id,
        term_start: 0,
        term_end: 0,
        assignment_source: entitybase::InstitutionAssignmentSource::InstitutionGovernance,
        assignment_source_ref: b"proposal-result".to_vec(),
        assignment_status: entitybase::InstitutionAssignmentStatus::Active,
    }
}

#[test]
fn private_dynamic_role_is_authorized_only_for_assigned_admin() {
    new_test_ext().execute_with(|| {
        use entitybase::{
            InstitutionRoleAuthorizationQuery, InstitutionRoleMutation, RolePermissionOperation,
            RoleSubject,
        };

        let cid_number = generated_cid("private-role", "SFLP");
        let code = code_bytes("SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code,
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                (crate::RESERVED_NAME_FEE, 0),
            ]),
        ));
        let action = entitybase::BusinessActionId {
            module_tag: b"pri-mgmt".to_vec(),
            action_code: 3,
        };
        let role_nonce = pallet::InstitutionRoleNonce::<Test>::get(&cid_number);
        assert_ok!(PrivateManage::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: code,
                cid_number: cid_number.to_vec(),
                proposal_id: 51,
                role_mutations: vec![InstitutionRoleMutation::Create {
                    role_name: "财务负责人".as_bytes().to_vec(),
                    term_required: false,
                    permissions: vec![entitybase::RolePermissionSpec {
                        business_action_id: action.clone(),
                        operation: RolePermissionOperation::Propose,
                    }],
                    assignments: vec![governance_assignment(admin(1))],
                }],
                assignment_changes: vec![],
                legal_representative_change: None,
                result_source_ref: b"proposal-51".to_vec(),
            }
        ));

        let role_code = entitybase::generate_dynamic_role_code(
            crate::MODULE_TAG,
            cid_number.as_slice(),
            role_nonce,
            51,
        );
        let subject = RoleSubject {
            cid_number: cid_number.to_vec(),
            role_code,
        };
        assert!(<PrivateManage as InstitutionRoleAuthorizationQuery<
            AccountId32,
        >>::is_authorized(
            &admin(1),
            &subject,
            &action,
            RolePermissionOperation::Propose,
        ));
        assert!(!<PrivateManage as InstitutionRoleAuthorizationQuery<
            AccountId32,
        >>::is_authorized(
            &admin(2),
            &subject,
            &action,
            RolePermissionOperation::Propose,
        ));
    });
}

#[test]
fn initialized_lr_wallet_is_independent_of_personal_identity_record() {
    new_test_ext().execute_with(|| {
        use entitybase::{
            InstitutionLegalRepresentativeChange, InstitutionRoleAssignmentChange,
            InstitutionRoleAuthorizationQuery, InstitutionRoleMutation, RolePermissionOperation,
            RoleSubject,
        };

        let cid_number = generated_cid("private-lr-fallback", "SFLP");
        let code = code_bytes("SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code,
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                (crate::RESERVED_NAME_FEE, 0),
            ]),
        ));
        // 名册保持 create_institution 默认：admin(1)/admin(2) 均【不带 cid】
        //（分层规则：私权名册 cid 不强制），LR 的四要素只落在 InstitutionInfo 身份记录上。
        let citizen_cid = b"CN220-CTZN2-198805200-2026".to_vec();
        let lr_code = primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE.to_vec();
        assert_ok!(PrivateManage::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: code,
                cid_number: cid_number.to_vec(),
                proposal_id: 80,
                role_mutations: vec![],
                assignment_changes: vec![InstitutionRoleAssignmentChange {
                    role_code: lr_code,
                    assignments: vec![governance_assignment(admin(1))],
                }],
                legal_representative_change: Some(InstitutionLegalRepresentativeChange::Set {
                    family_name: "张".as_bytes().to_vec(),
                    given_name: "三".as_bytes().to_vec(),
                    cid_number: citizen_cid.clone(),
                    account_id: admin(1),
                }),
                result_source_ref: b"proposal-80".to_vec(),
            }
        ));

        // 业务岗位：LR 本人 admin(1) 与非 LR 的 admin(2) 同时任职。
        let action = entitybase::BusinessActionId {
            module_tag: b"pri-mgmt".to_vec(),
            action_code: 3,
        };
        let role_nonce = pallet::InstitutionRoleNonce::<Test>::get(&cid_number);
        assert_ok!(PrivateManage::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: code,
                cid_number: cid_number.to_vec(),
                proposal_id: 81,
                role_mutations: vec![InstitutionRoleMutation::Create {
                    role_name: "业务负责人".as_bytes().to_vec(),
                    term_required: false,
                    permissions: vec![entitybase::RolePermissionSpec {
                        business_action_id: action.clone(),
                        operation: RolePermissionOperation::Propose,
                    }],
                    assignments: vec![
                        governance_assignment(admin(1)),
                        governance_assignment(admin(2)),
                    ],
                }],
                assignment_changes: vec![],
                legal_representative_change: None,
                result_source_ref: b"proposal-81".to_vec(),
            }
        ));
        let role_code = entitybase::generate_dynamic_role_code(
            crate::MODULE_TAG,
            cid_number.as_slice(),
            role_nonce,
            81,
        );
        let subject = RoleSubject {
            cid_number: cid_number.to_vec(),
            role_code,
        };
        let authorized = |who: &AccountId32| {
            <PrivateManage as InstitutionRoleAuthorizationQuery<AccountId32>>::is_authorized(
                who,
                &subject,
                &action,
                RolePermissionOperation::Propose,
            )
        };

        // 创世期：按 account_id。
        assert!(authorized(&admin(1)));

        // 法定代表人的资料记录不作为机构钱包来源；即使地址改变也不能授予该钱包业务权限。
        set_operation_phase(true);
        pallet::Institutions::<Test>::mutate(&cid_number, |info| {
            info.as_mut()
                .unwrap()
                .legal_representative
                .as_mut()
                .unwrap()
                .account_id = admin(5);
        });
        assert!(!authorized(&admin(5))); // LR资料不能替代当前钱包授权
        assert!(authorized(&admin(1))); // 机构钱包仍有效
        assert!(authorized(&admin(2))); // 非LR管理员仍使用自己的实际钱包
    });
}

#[test]
fn institution_wallet_authority_ignores_personal_cid_display_metadata() {
    new_test_ext().execute_with(|| {
        use entitybase::{
            InstitutionRoleAuthorizationQuery, InstitutionRoleMutation, RolePermissionOperation,
            RoleSubject,
        };

        let cid_number = generated_cid("private-rebind", "SFLP");
        let code = code_bytes("SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code,
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                (crate::RESERVED_NAME_FEE, 0),
            ]),
        ));

        // 用带 CID 的名册覆盖：admin(1) 携 CID、admin(2) 无 CID（privateadmin 无 seed 期绑定校验）。
        let citizen_cid = b"CN220-CTZN2-198805200-2026".to_vec();
        let admins: crate::InstitutionAdminsInputOf<Test> = vec![
            adminbase::Admin {
                account_id: admin(1),
                cid_number: citizen_cid.clone().try_into().expect("cid fits"),
                family_name: "张".as_bytes().to_vec().try_into().expect("family fits"),
                given_name: "三".as_bytes().to_vec().try_into().expect("given fits"),
            },
            adminbase::Admin {
                account_id: admin(2),
                cid_number: Default::default(),
                family_name: "管理".as_bytes().to_vec().try_into().expect("family fits"),
                given_name: "员".as_bytes().to_vec().try_into().expect("given fits"),
            },
        ]
        .try_into()
        .expect("admins fit");
        assert_ok!(seed_institution_admins(&cid_number, code, &admins));

        let action = entitybase::BusinessActionId {
            module_tag: b"pri-mgmt".to_vec(),
            action_code: 3,
        };
        let role_nonce = pallet::InstitutionRoleNonce::<Test>::get(&cid_number);
        assert_ok!(PrivateManage::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: code,
                cid_number: cid_number.to_vec(),
                proposal_id: 70,
                role_mutations: vec![InstitutionRoleMutation::Create {
                    role_name: "业务负责人".as_bytes().to_vec(),
                    term_required: false,
                    permissions: vec![entitybase::RolePermissionSpec {
                        business_action_id: action.clone(),
                        operation: RolePermissionOperation::Propose,
                    }],
                    assignments: vec![
                        governance_assignment(admin(1)),
                        governance_assignment(admin(2)),
                    ],
                }],
                assignment_changes: vec![],
                legal_representative_change: None,
                result_source_ref: b"proposal-70".to_vec(),
            }
        ));
        let role_code = entitybase::generate_dynamic_role_code(
            crate::MODULE_TAG,
            cid_number.as_slice(),
            role_nonce,
            70,
        );
        let subject = RoleSubject {
            cid_number: cid_number.to_vec(),
            role_code,
        };
        let authorized = |who: &AccountId32| {
            <PrivateManage as InstitutionRoleAuthorizationQuery<AccountId32>>::is_authorized(
                who,
                &subject,
                &action,
                RolePermissionOperation::Propose,
            )
        };

        // 创世期：按 account_id 授权。
        assert!(authorized(&admin(1)));
        assert!(!authorized(&admin(5)));

        // 运行期调整名册的个人CID资料，不会将其他钱包变成机构管理员。
        set_operation_phase(true);
        let key = adminbase::AdminCidNumber::try_from(cid_number.to_vec()).unwrap();
        privateadmin::AdminAccounts::<Test>::mutate(&key, |roster| {
            roster.as_mut().unwrap().admins[0].cid_number =
                b"CN221-CTZN2-198805200-2026".to_vec().try_into().unwrap();
        });
        assert!(!authorized(&admin(5))); // 身份资料不能授予其他钱包机构权限
        assert!(authorized(&admin(1))); // 机构钱包保持原授权
        assert!(authorized(&admin(2))); // 无 CID 管理员运行期仍按 account_id
    });
}

#[test]
fn private_legal_representative_role_is_unique_and_allows_zero_or_one_assignment() {
    new_test_ext().execute_with(|| {
        use entitybase::{
            InstitutionLegalRepresentativeChange, InstitutionRoleAssignmentChange,
            InstitutionRoleMutation, RolePermissionOperation,
        };

        let cid_number = generated_cid("private-lr", "SFLP");
        let code = code_bytes("SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code,
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                (crate::RESERVED_NAME_FEE, 0),
            ]),
        ));

        assert_noop!(
            PrivateManage::apply_institution_governance_result(
                entitybase::InstitutionGovernanceResult {
                    institution_code: code,
                    cid_number: cid_number.to_vec(),
                    proposal_id: 52,
                    role_mutations: vec![InstitutionRoleMutation::Create {
                        role_name:
                            primitives::institution_constraints::ROLE_NAME_LEGAL_REPRESENTATIVE
                                .to_vec(),
                        term_required: false,
                        permissions: vec![entitybase::RolePermissionSpec {
                            business_action_id: entitybase::BusinessActionId {
                                module_tag: b"pri-mgmt".to_vec(),
                                action_code: 3,
                            },
                            operation: RolePermissionOperation::Propose,
                        }],
                        assignments: vec![],
                    }],
                    assignment_changes: vec![],
                    legal_representative_change: None,
                    result_source_ref: b"proposal-52".to_vec(),
                }
            ),
            Error::<Test>::DuplicateRoleName
        );

        let lr_code = primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE.to_vec();
        assert_ok!(PrivateManage::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: code,
                cid_number: cid_number.to_vec(),
                proposal_id: 53,
                role_mutations: vec![],
                assignment_changes: vec![InstitutionRoleAssignmentChange {
                    role_code: lr_code.clone(),
                    assignments: vec![governance_assignment(admin(1))],
                }],
                legal_representative_change: Some(InstitutionLegalRepresentativeChange::Set {
                    family_name: "张".as_bytes().to_vec(),
                    given_name: "三".as_bytes().to_vec(),
                    cid_number: b"CITIZEN-LR-PRIVATE".to_vec(),
                    account_id: admin(1),
                }),
                result_source_ref: b"proposal-53".to_vec(),
            }
        ));
        assert_eq!(
            pallet::InstitutionRoleAssignments::<Test>::get(
                &cid_number,
                crate::RoleCodeOf::try_from(lr_code.clone()).expect("LR code fits"),
            )
            .len(),
            1
        );

        assert_ok!(PrivateManage::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: code,
                cid_number: cid_number.to_vec(),
                proposal_id: 54,
                role_mutations: vec![],
                assignment_changes: vec![InstitutionRoleAssignmentChange {
                    role_code: lr_code,
                    assignments: vec![],
                }],
                legal_representative_change: Some(InstitutionLegalRepresentativeChange::Clear),
                result_source_ref: b"proposal-54".to_vec(),
            }
        ));
        let institution =
            pallet::Institutions::<Test>::get(&cid_number).expect("private institution remains");
        assert!(institution.legal_representative.is_none());
    });
}

#[test]
fn genesis_phase_allows_empty_legal_representative() {
    new_test_ext().execute_with(|| {
        use entitybase::{InstitutionLegalRepresentativeChange, InstitutionRoleAssignmentChange};
        let cid_number = generated_cid("private-lr-genesis", "SFLP");
        let code = code_bytes("SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code,
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                (crate::RESERVED_NAME_FEE, 0),
            ]),
        ));
        let lr_code = primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE.to_vec();

        // Genesis(创世/开发期):LR 岗四要素允许为空。
        assert_ok!(PrivateManage::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: code,
                cid_number: cid_number.to_vec(),
                proposal_id: 60,
                role_mutations: vec![],
                assignment_changes: vec![InstitutionRoleAssignmentChange {
                    role_code: lr_code,
                    assignments: vec![governance_assignment(admin(1))],
                }],
                legal_representative_change: Some(InstitutionLegalRepresentativeChange::Set {
                    family_name: vec![],
                    given_name: vec![],
                    cid_number: vec![],
                    account_id: admin(1),
                }),
                result_source_ref: b"proposal-60".to_vec(),
            }
        ));
    });
}

#[test]
fn operation_phase_requires_complete_legal_representative() {
    new_test_ext().execute_with(|| {
        use entitybase::{InstitutionLegalRepresentativeChange, InstitutionRoleAssignmentChange};
        let cid_number = generated_cid("private-lr-operation", "SFLP");
        let code = code_bytes("SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code,
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                (crate::RESERVED_NAME_FEE, 0),
            ]),
        ));
        let lr_code = primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE.to_vec();
        set_operation_phase(true);

        // 运行期:空姓名 → 拒。
        assert_noop!(
            PrivateManage::apply_institution_governance_result(
                entitybase::InstitutionGovernanceResult {
                    institution_code: code,
                    cid_number: cid_number.to_vec(),
                    proposal_id: 61,
                    role_mutations: vec![],
                    assignment_changes: vec![InstitutionRoleAssignmentChange {
                        role_code: lr_code.clone(),
                        assignments: vec![governance_assignment(admin(1))],
                    }],
                    legal_representative_change: Some(InstitutionLegalRepresentativeChange::Set {
                        family_name: vec![],
                        given_name: "三".as_bytes().to_vec(),
                        cid_number: b"CITIZEN-LR-PRIVATE".to_vec(),
                        account_id: admin(1),
                    }),
                    result_source_ref: b"proposal-61".to_vec(),
                }
            ),
            Error::<Test>::EmptyLegalRepresentativeName
        );

        // 运行期:空 CID → 拒。
        assert_noop!(
            PrivateManage::apply_institution_governance_result(
                entitybase::InstitutionGovernanceResult {
                    institution_code: code,
                    cid_number: cid_number.to_vec(),
                    proposal_id: 62,
                    role_mutations: vec![],
                    assignment_changes: vec![InstitutionRoleAssignmentChange {
                        role_code: lr_code.clone(),
                        assignments: vec![governance_assignment(admin(1))],
                    }],
                    legal_representative_change: Some(InstitutionLegalRepresentativeChange::Set {
                        family_name: "张".as_bytes().to_vec(),
                        given_name: "三".as_bytes().to_vec(),
                        cid_number: vec![],
                        account_id: admin(1),
                    }),
                    result_source_ref: b"proposal-62".to_vec(),
                }
            ),
            Error::<Test>::EmptyLegalRepresentativeCidNumber
        );

        // 运行期:四要素齐全 → Ok。
        assert_ok!(PrivateManage::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: code,
                cid_number: cid_number.to_vec(),
                proposal_id: 63,
                role_mutations: vec![],
                assignment_changes: vec![InstitutionRoleAssignmentChange {
                    role_code: lr_code,
                    assignments: vec![governance_assignment(admin(1))],
                }],
                legal_representative_change: Some(InstitutionLegalRepresentativeChange::Set {
                    family_name: "张".as_bytes().to_vec(),
                    given_name: "三".as_bytes().to_vec(),
                    cid_number: b"CITIZEN-LR-PRIVATE".to_vec(),
                    account_id: admin(1),
                }),
                result_source_ref: b"proposal-63".to_vec(),
            }
        ));
    });
}

#[test]
fn create_uses_cid_as_the_only_institution_identity() {
    new_test_ext().execute_with(|| {
        let cid_number = generated_cid("private-create", "SFLP");
        let code = code_bytes("SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code,
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                (crate::RESERVED_NAME_FEE, 0)
            ]),
        ));

        let main_account = account_of(&cid_number, crate::RESERVED_NAME_MAIN);
        let fee_account = account_of(&cid_number, crate::RESERVED_NAME_FEE);
        assert_ne!(main_account, fee_account);
        assert!(pallet::Institutions::<Test>::contains_key(&cid_number));
        assert_eq!(
            pallet::InstitutionAccounts::<Test>::get(
                &cid_number,
                account_name(crate::RESERVED_NAME_MAIN)
            )
            .map(|item| item.account_id),
            Some(main_account.clone())
        );
        assert_eq!(
            pallet::AccountRegisteredCid::<Test>::get(&fee_account).map(|item| item.cid_number),
            Some(cid_number.clone())
        );

        // 管理员和投票阈值均按 CID 寻址，机构账户不再充当管理员根或阈值 key。
        assert_eq!(
            PrivateAdmins::institution_admins(code, cid_number.as_slice()),
            Some(alloc::vec![admin(1), admin(2)])
        );
        assert_eq!(
            InternalVote::active_institution_threshold(code, cid_number.as_slice()),
            Some(2)
        );
        assert!(pallet::InstitutionRoles::<Test>::contains_key(
            &cid_number,
            crate::RoleCodeOf::try_from(
                primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE.to_vec()
            )
            .expect("LR 岗位码必须受界")
        ));
        assert!(pallet::InstitutionRoleAssignments::<Test>::get(
            &cid_number,
            crate::RoleCodeOf::try_from(
                primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE.to_vec()
            )
            .expect("LR 岗位码必须受界")
        )
        .is_empty());
    });
}

#[test]
fn protocol_accounts_are_automatically_created_with_zero_balance() {
    new_test_ext().execute_with(|| {
        let zero_cid = generated_cid("private-zero", "SFLP");
        assert_ok!(create_institution(
            zero_cid.clone(),
            code_bytes("SFLP"),
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                (crate::RESERVED_NAME_FEE, 0)
            ]),
        ));
        assert_eq!(
            pallet::InstitutionAccounts::<Test>::get(
                &zero_cid,
                account_name(crate::RESERVED_NAME_MAIN)
            )
            .expect("主账户必须存在")
            .initial_balance,
            0
        );

        assert_eq!(
            pallet::InstitutionAccounts::<Test>::get(
                &zero_cid,
                account_name(crate::RESERVED_NAME_FEE)
            )
            .expect("费用账户必须存在")
            .initial_balance,
            0
        );
    });
}

#[test]
fn creation_derives_protocol_accounts_without_client_account_input() {
    new_test_ext().execute_with(|| {
        let cid_number = generated_cid("private-missing-fee", "SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code_bytes("SFLP"),
            initial_accounts(&[]),
        ));
        assert!(pallet::InstitutionAccounts::<Test>::contains_key(
            &cid_number,
            account_name(crate::RESERVED_NAME_MAIN)
        ));
        assert!(pallet::InstitutionAccounts::<Test>::contains_key(
            &cid_number,
            account_name(crate::RESERVED_NAME_FEE)
        ));
    });
}

#[test]
fn update_and_add_account_keep_cid_as_the_target_key() {
    new_test_ext().execute_with(|| {
        let cid_number = generated_cid("private-maintain", "SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code_bytes("SFLP"),
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                (crate::RESERVED_NAME_FEE, 0)
            ]),
        ));

        assert_ok!(PrivateManage::update_institution_info(
            RuntimeOrigin::signed(registrar()),
            cid_number.clone(),
            account_name("更新后的机构全称".as_bytes()),
            account_name("更新简称".as_bytes()),
            b"GD001-FRG00-000000001-2026".to_vec(),
            b"REGISTRY-ROLE".to_vec(),
        ));
        assert_eq!(
            pallet::Institutions::<Test>::get(&cid_number)
                .expect("机构必须存在")
                .cid_short_name,
            account_name("更新简称".as_bytes())
        );

        // 新增账户改为本机构提案 → 内部投票通过 → finalizer 落库。
        assert_ok!(propose_add_custom_account(
            RuntimeOrigin::signed(admin(1)),
            cid_number.clone(),
            &["专项账户".as_bytes()],
        ));
        let proposal_id = VotingEngine::next_proposal_id().saturating_sub(1);
        assert_ok!(cast_yes_votes(proposal_id));
        let named_account = account_of(&cid_number, "专项账户".as_bytes());
        assert_eq!(
            pallet::AccountRegisteredCid::<Test>::get(named_account)
                .map(|item| (item.cid_number, item.account_name)),
            Some((cid_number, account_name("专项账户".as_bytes())))
        );
    });
}

#[test]
fn add_account_proposal_then_vote_inserts_account() {
    new_test_ext().execute_with(|| {
        let cid_number = generated_cid("private-add-vote", "SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code_bytes("SFLP"),
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                (crate::RESERVED_NAME_FEE, 0)
            ]),
        ));
        let new_name = "投票新增账户".as_bytes();
        assert!(!pallet::InstitutionAccounts::<Test>::contains_key(
            &cid_number,
            account_name(new_name),
        ));

        assert_ok!(propose_add_custom_account(
            RuntimeOrigin::signed(admin(1)),
            cid_number.clone(),
            &[new_name],
        ));
        let proposal_id = VotingEngine::next_proposal_id().saturating_sub(1);
        // 发起后 Pending 命中,尚未落库。
        assert_eq!(
            pallet::InstitutionPendingAdd::<Test>::get(&cid_number),
            Some(proposal_id)
        );
        assert!(!pallet::InstitutionAccounts::<Test>::contains_key(
            &cid_number,
            account_name(new_name),
        ));

        assert_ok!(cast_yes_votes(proposal_id));

        // 通过后账户落库、反向索引写入、Pending 清除。
        let added = account_of(&cid_number, new_name);
        assert!(
            sp_io::storage::exists(&codec::Encode::encode(&(
                b"evm-receive-registration",
                &added
            ))),
            "zero-balance business registration must invoke the receive-alias hook"
        );
        assert_eq!(
            pallet::AccountRegisteredCid::<Test>::get(&added).map(|item| item.cid_number),
            Some(cid_number.clone())
        );
        assert!(!pallet::InstitutionPendingAdd::<Test>::contains_key(
            &cid_number
        ));
    });
}

#[test]
fn add_account_requires_institution_admin_and_role() {
    new_test_ext().execute_with(|| {
        let cid_number = generated_cid("private-add-auth", "SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code_bytes("SFLP"),
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                (crate::RESERVED_NAME_FEE, 0)
            ]),
        ));
        // 非本机构管理员账户发起 → build_institution_vote_plan 授权失败。
        assert_noop!(
            propose_add_custom_account(
                RuntimeOrigin::signed(admin(9)),
                cid_number.clone(),
                &["越权账户".as_bytes()],
            ),
            pallet::Error::<Test>::PermissionDenied
        );
        // 不存在的机构 → InstitutionNotFound。
        let ghost = generated_cid("private-add-ghost", "SFLP");
        assert_noop!(
            propose_add_custom_account(
                RuntimeOrigin::signed(admin(1)),
                ghost,
                &["幽灵账户".as_bytes()],
            ),
            pallet::Error::<Test>::InstitutionNotFound
        );
    });
}

#[test]
fn add_account_rejects_protocol_names_and_duplicate_custom_names() {
    new_test_ext().execute_with(|| {
        let cid_number = generated_cid("private-add-invalid", "SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code_bytes("SFLP"),
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                (crate::RESERVED_NAME_FEE, 0)
            ]),
        ));
        // 保留名/重复名在发起阶段即被派生校验链拒绝,不写 Pending、不建提案。
        assert_noop!(
            propose_add_custom_account(
                RuntimeOrigin::signed(admin(1)),
                cid_number.clone(),
                &[crate::RESERVED_NAME_MAIN],
            ),
            pallet::Error::<Test>::ReservedAccountName
        );
        assert_noop!(
            propose_add_custom_account(
                RuntimeOrigin::signed(admin(1)),
                cid_number,
                &["重复账户".as_bytes(), "重复账户".as_bytes()],
            ),
            pallet::Error::<Test>::DuplicateAccountName
        );
    });
}

#[test]
fn duplicate_add_proposal_is_rejected_while_pending() {
    new_test_ext().execute_with(|| {
        let cid_number = generated_cid("private-add-pending", "SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code_bytes("SFLP"),
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                (crate::RESERVED_NAME_FEE, 0)
            ]),
        ));
        // 首次发起成功后 InstitutionPendingAdd 命中,同机构重复发起新增必须被拒。
        assert_ok!(propose_add_custom_account(
            RuntimeOrigin::signed(admin(1)),
            cid_number.clone(),
            &["账户甲".as_bytes()],
        ));
        assert_noop!(
            propose_add_custom_account(
                RuntimeOrigin::signed(admin(1)),
                cid_number,
                &["账户乙".as_bytes()],
            ),
            pallet::Error::<Test>::AddAlreadyPending
        );
    });
}

#[test]
fn rejected_add_is_cleaned_only_by_engingcore_callback() {
    new_test_ext().execute_with(|| {
        let cid_number = generated_cid("private-add-rejected", "SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code_bytes("SFLP"),
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                (crate::RESERVED_NAME_FEE, 0)
            ]),
        ));
        let new_name = "被否新增账户".as_bytes();
        assert_ok!(propose_add_custom_account(
            RuntimeOrigin::signed(admin(1)),
            cid_number.clone(),
            &[new_name],
        ));
        let proposal_id = VotingEngine::next_proposal_id().saturating_sub(1);
        assert_eq!(
            pallet::InstitutionPendingAdd::<Test>::get(&cid_number),
            Some(proposal_id)
        );

        assert_eq!(
            <crate::InternalVoteExecutor<Test> as engingcore::InternalVoteResultCallback>::on_internal_vote_finalized(
                proposal_id,
                false,
            ),
            Ok(engingcore::ProposalExecutionOutcome::Executed)
        );
        // 否决由投票引擎回调清 Pending,账户不落库。
        assert!(!pallet::InstitutionPendingAdd::<Test>::contains_key(
            &cid_number
        ));
        assert!(!pallet::InstitutionAccounts::<Test>::contains_key(
            &cid_number,
            account_name(new_name),
        ));
    });
}

#[test]
fn only_named_account_can_be_closed_and_institution_stays_alive() {
    new_test_ext().execute_with(|| {
        let cid_number = generated_cid("private-close-named", "SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code_bytes("SFLP"),
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                // 关闭操作的链上费必须由本机构费用账户支付；余额不足时不得回落管理员。
                (crate::RESERVED_NAME_FEE, 1_000),
                ("项目账户".as_bytes(), 1_000),
            ]),
        ));
        let named_account = account_of(&cid_number, "项目账户".as_bytes());
        let fee_account = account_of(&cid_number, crate::RESERVED_NAME_FEE);
        let admin_balance_before = Balances::free_balance(admin(1));

        assert_ok!(PrivateManage::propose_close_private_institution(
            RuntimeOrigin::signed(admin(1)),
            cid_number.clone(),
            b"TEST_CLOSE_ROLE".to_vec().try_into().expect("role fits"),
            named_account.clone(),
            beneficiary_account_id(),
        ));
        let proposal_id = VotingEngine::next_proposal_id().saturating_sub(1);
        assert_ok!(cast_yes_votes(proposal_id));

        assert_eq!(Balances::free_balance(&fee_account), 990);
        assert_eq!(Balances::free_balance(beneficiary_account_id()), 1_100);
        assert_eq!(Balances::free_balance(admin(1)), admin_balance_before);
        assert!(!pallet::AccountRegisteredCid::<Test>::contains_key(
            &named_account
        ));
        assert!(!pallet::InstitutionAccounts::<Test>::contains_key(
            &cid_number,
            account_name("项目账户".as_bytes())
        ));
        assert!(pallet::Institutions::<Test>::contains_key(&cid_number));
        assert!(pallet::InstitutionAccounts::<Test>::contains_key(
            &cid_number,
            account_name(crate::RESERVED_NAME_MAIN)
        ));
        assert_eq!(
            PrivateAdmins::institution_admins(code_bytes("SFLP"), cid_number.as_slice()),
            Some(alloc::vec![admin(1), admin(2)])
        );
    });
}

#[test]
fn rejected_close_is_cleaned_only_by_engingcore_callback() {
    new_test_ext().execute_with(|| {
        let cid_number = generated_cid("private-close-rejected", "SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code_bytes("SFLP"),
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                (crate::RESERVED_NAME_FEE, 1_000),
                ("项目账户".as_bytes(), 1_000),
            ]),
        ));
        let named_account = account_of(&cid_number, "项目账户".as_bytes());

        assert_ok!(PrivateManage::propose_close_private_institution(
            RuntimeOrigin::signed(admin(1)),
            cid_number.clone(),
            b"TEST_CLOSE_ROLE".to_vec().try_into().expect("role fits"),
            named_account.clone(),
            beneficiary_account_id(),
        ));
        let proposal_id = VotingEngine::next_proposal_id().saturating_sub(1);
        assert_eq!(
            pallet::InstitutionPendingClose::<Test>::get(&named_account),
            Some(proposal_id)
        );

        assert_eq!(
            <crate::InternalVoteExecutor<Test> as engingcore::InternalVoteResultCallback>::on_internal_vote_finalized(
                proposal_id,
                false,
            ),
            Ok(engingcore::ProposalExecutionOutcome::Executed)
        );
        assert!(!pallet::InstitutionPendingClose::<Test>::contains_key(
            &named_account
        ));
        assert!(pallet::InstitutionAccounts::<Test>::contains_key(
            &cid_number,
            account_name("项目账户".as_bytes()),
        ));
    });
}

#[test]
fn protocol_account_close_is_rejected() {
    new_test_ext().execute_with(|| {
        let cid_number = generated_cid("private-close-main", "SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code_bytes("SFLP"),
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                (crate::RESERVED_NAME_FEE, 0)
            ]),
        ));
        let main_account = account_of(&cid_number, crate::RESERVED_NAME_MAIN);

        assert_noop!(
            PrivateManage::propose_close_private_institution(
                RuntimeOrigin::signed(admin(1)),
                cid_number.clone(),
                b"TEST_CLOSE_ROLE".to_vec().try_into().expect("role fits"),
                main_account,
                beneficiary_account_id(),
            ),
            pallet::Error::<Test>::CannotCloseProtectedInstitution
        );
    });
}

#[test]
fn account_operation_rejects_actor_cid_mismatch() {
    new_test_ext().execute_with(|| {
        let cid_number = generated_cid("private-actor", "SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code_bytes("SFLP"),
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                (crate::RESERVED_NAME_FEE, 0),
                ("项目账户".as_bytes(), 0),
            ]),
        ));
        let named_account = account_of(&cid_number, "项目账户".as_bytes());
        let other_cid = generated_cid("private-other-actor", "SFLP");

        assert_noop!(
            PrivateManage::propose_close_private_institution(
                RuntimeOrigin::signed(admin(1)),
                other_cid,
                crate::RoleCodeOf::default(),
                named_account,
                beneficiary_account_id(),
            ),
            pallet::Error::<Test>::NotInstitutionAccount
        );
    });
}

#[test]
fn non_admin_cannot_start_institution_account_close() {
    new_test_ext().execute_with(|| {
        let cid_number = generated_cid("private-close-auth", "SFLP");
        assert_ok!(create_institution(
            cid_number.clone(),
            code_bytes("SFLP"),
            initial_accounts(&[
                (crate::RESERVED_NAME_MAIN, 0),
                (crate::RESERVED_NAME_FEE, 0),
                ("项目账户".as_bytes(), 0),
            ]),
        ));
        let named_account = account_of(&cid_number, "项目账户".as_bytes());

        assert_noop!(
            PrivateManage::propose_close_private_institution(
                RuntimeOrigin::signed(admin(8)),
                cid_number.clone(),
                b"TEST_CLOSE_ROLE".to_vec().try_into().expect("role fits"),
                named_account,
                beneficiary_account_id(),
            ),
            pallet::Error::<Test>::PermissionDenied
        );
    });
}

// 登记沿用统一机构模型。夹具在原测试文件内建立，不新增固定岗位/测试资产。
fn fresh_registration_target(tag: &str) -> pallet::CidNumberOf<Test> {
    let cid = generated_cid(tag, "SFLP");
    assert_ok!(create_institution(
        cid.clone(),
        code_bytes("SFLP"),
        initial_accounts(&[
            (crate::RESERVED_NAME_MAIN, 0),
            (crate::RESERVED_NAME_FEE, 0)
        ])
    ));
    pallet::InstitutionGovernanceThresholds::<Test>::remove(&cid);
    let roles = pallet::InstitutionRoles::<Test>::iter_prefix(&cid)
        .map(|(code, _)| code)
        .collect::<Vec<_>>();
    for code in roles {
        if code.as_slice() != b"LR" {
            pallet::InstitutionRoles::<Test>::remove(&cid, &code);
        }
        pallet::InstitutionRolePermissions::<Test>::remove(&cid, &code);
        pallet::InstitutionRoleAssignments::<Test>::remove(&cid, &code);
    }
    privateadmin::AdminAccounts::<Test>::remove(&cid);
    let _ = privateadmin::AdminWallets::<Test>::clear_prefix(&cid, u32::MAX, None);
    let _ = privateadmin::WalletRoleBindings::<Test>::clear_prefix(&cid, u32::MAX, None);
    pallet::InstitutionRoleNonce::<Test>::remove(&cid);
    cid
}

fn registration_people(count: u8) -> Vec<adminbase::Admin<AccountId32>> {
    (1..=count)
        .map(|index| adminbase::Admin {
            account_id: admin(index),
            cid_number: Default::default(),
            family_name: b"F".to_vec().try_into().unwrap(),
            given_name: b"G".to_vec().try_into().unwrap(),
        })
        .collect()
}

fn registration_target(
    account: AccountId32,
) -> entitybase::InstitutionAssignmentTarget<AccountId32> {
    entitybase::InstitutionAssignmentTarget {
        account_id: account,
        term_start: 0,
        term_end: 0,
        assignment_source: entitybase::InstitutionAssignmentSource::Registry,
        assignment_source_ref: Vec::new(),
        assignment_status: entitybase::InstitutionAssignmentStatus::Active,
    }
}

fn initial_registration_action() -> crate::pallet::InstitutionGovernanceActionOf<Test> {
    let permission = |action_code, operation| entitybase::RolePermissionSpec {
        business_action_id: entitybase::BusinessActionId {
            module_tag: crate::MODULE_TAG.to_vec(),
            action_code,
        },
        operation,
    };
    entitybase::InstitutionGovernanceAction::MutateRolesAndAssignments {
        role_mutations: vec![
            entitybase::InstitutionRoleMutation::Create {
                role_name: "治理岗位".as_bytes().to_vec(),
                term_required: false,
                permissions: vec![
                    permission(3, RolePermissionOperation::Propose),
                    permission(3, RolePermissionOperation::Vote),
                ],
                assignments: vec![registration_target(admin(1)), registration_target(admin(2))],
            },
            entitybase::InstitutionRoleMutation::Create {
                role_name: "业务岗位".as_bytes().to_vec(),
                term_required: false,
                permissions: vec![permission(2, RolePermissionOperation::Propose)],
                assignments: (1..=3)
                    .map(|index| registration_target(admin(index)))
                    .collect(),
            },
        ],
        assignment_changes: Vec::new(),
        legal_representative_change: None,
    }
}

fn send_registration(
    cid: &pallet::CidNumberOf<Test>,
    action: crate::pallet::InstitutionGovernanceActionOf<Test>,
    expected: [u8; 32],
    threshold: Option<u32>,
) -> sp_runtime::DispatchResult {
    PrivateManage::register_institution_admins(
        RuntimeOrigin::signed(registrar()),
        cid.clone(),
        action,
        b"GD001-FRG00-000000001-2026".to_vec(),
        b"REGISTRY-ROLE".to_vec(),
        expected,
        threshold,
    )
}

#[test]
fn wallet_registration_bootstraps_without_roles_and_can_remove_last_wallet() {
    new_test_ext().execute_with(|| {
        let cid = fresh_registration_target("wallet-reg-independent");
        let before_roles = pallet::InstitutionRoles::<Test>::iter_prefix(&cid)
            .collect::<Vec<_>>()
            .encode();
        let before_assignments = pallet::InstitutionRoleAssignments::<Test>::iter_prefix(&cid)
            .collect::<Vec<_>>()
            .encode();
        let before_accounts = account_state_without_wallet_refs().encode();
        let action = entitybase::InstitutionGovernanceAction::RegisterAdminWalletChanges {
            added: registration_people(3),
            removed: vec![],
            replaced: vec![],
        };
        let hash = PrivateManage::registration_state_hash(&cid).unwrap();
        assert_ok!(send_registration(&cid, action.clone(), hash, None));
        assert_noop!(
            send_registration(&cid, action, hash, None),
            Error::<Test>::StaleRegistrationState
        );
        assert_eq!(
            pallet::InstitutionGovernanceThresholds::<Test>::get(&cid),
            None
        );
        assert_eq!(
            pallet::InstitutionRoles::<Test>::iter_prefix(&cid)
                .collect::<Vec<_>>()
                .encode(),
            before_roles
        );
        assert_eq!(
            pallet::InstitutionRoleAssignments::<Test>::iter_prefix(&cid)
                .collect::<Vec<_>>()
                .encode(),
            before_assignments
        );
        assert_eq!(
            account_state_without_wallet_refs().encode(),
            before_accounts
        );
        // Explicit governance configuration is a separate signed operation.
        assert_ok!(send_registration(
            &cid,
            initial_registration_action(),
            PrivateManage::registration_state_hash(&cid).unwrap(),
            Some(2)
        ));
        let assignments = pallet::InstitutionRoleAssignments::<Test>::iter_prefix(&cid)
            .collect::<Vec<_>>()
            .encode();
        let remove = entitybase::InstitutionGovernanceAction::RegisterAdminWalletChanges {
            added: vec![],
            removed: (1..=3).map(|i| (admin(i), 0)).collect(),
            replaced: vec![],
        };
        assert_ok!(send_registration(
            &cid,
            remove,
            PrivateManage::registration_state_hash(&cid).unwrap(),
            None
        ));
        assert_eq!(
            privateadmin::AdminAccounts::<Test>::get(&cid)
                .unwrap()
                .admins
                .len(),
            0
        );
        assert_eq!(
            pallet::InstitutionGovernanceThresholds::<Test>::get(&cid),
            Some(2)
        );
        assert_eq!(
            pallet::InstitutionRoleAssignments::<Test>::iter_prefix(&cid)
                .collect::<Vec<_>>()
                .encode(),
            assignments
        );
        let add = entitybase::InstitutionGovernanceAction::RegisterAdminWalletChanges {
            added: registration_people(1),
            removed: vec![],
            replaced: vec![],
        };
        assert_ok!(send_registration(
            &cid,
            add,
            PrivateManage::registration_state_hash(&cid).unwrap(),
            None
        ));
        assert_eq!(
            privateadmin::AdminWallets::<Test>::get(&cid, admin(1))
                .unwrap()
                .revision,
            2
        );
    });
}

#[test]
fn registry_wallet_replacement_preserves_personnel_and_rejects_stale_or_mixed_operations_atomically(
) {
    use adminbase::InstitutionAdminQuery;
    new_test_ext().execute_with(|| {
        let cid = fresh_registration_target("wallet-reg-rotate");
        let seed = entitybase::InstitutionGovernanceAction::RegisterAdminWalletChanges {
            added: registration_people(3),
            removed: vec![],
            replaced: vec![],
        };
        assert_ok!(send_registration(
            &cid,
            seed,
            PrivateManage::registration_state_hash(&cid).unwrap(),
            None
        ));
        assert_ok!(send_registration(
            &cid,
            initial_registration_action(),
            PrivateManage::registration_state_hash(&cid).unwrap(),
            Some(2)
        ));
        let people = pallet::InstitutionRoleAssignments::<Test>::iter_prefix(&cid)
            .collect::<Vec<_>>()
            .encode();
        let balances = account_state_without_wallet_refs().encode();
        let change = adminbase::InstitutionAdminWalletChange {
            admin_account: admin(1),
            expected_wallet: admin(1),
            new_wallet: admin(91),
            expected_revision: 0,
        };
        let action = entitybase::InstitutionGovernanceAction::RegisterAdminWalletChanges {
            added: vec![],
            removed: vec![],
            replaced: vec![change.clone()],
        };
        assert_ok!(send_registration(
            &cid,
            action,
            PrivateManage::registration_state_hash(&cid).unwrap(),
            None
        ));
        let code = pallet::Institutions::<Test>::get(&cid)
            .unwrap()
            .institution_code;
        assert_eq!(
            privateadmin::Pallet::<Test>::resolve_admin_account(code, &cid, &admin(91)),
            Some(admin(91))
        );
        assert_eq!(
            privateadmin::Pallet::<Test>::resolve_admin_account(code, &cid, &admin(1)),
            None
        );
        assert_eq!(
            privateadmin::WalletRoleBindings::<Test>::get(&cid, admin(91)),
            Some(admin(1))
        );
        assert_eq!(
            pallet::InstitutionRoleAssignments::<Test>::iter_prefix(&cid)
                .collect::<Vec<_>>()
                .encode(),
            people
        );
        assert_eq!(account_state_without_wallet_refs().encode(), balances);
        let bad = entitybase::InstitutionGovernanceAction::RegisterAdminWalletChanges {
            added: vec![registration_people(4).pop().unwrap()],
            removed: vec![],
            replaced: vec![change],
        };
        assert!(send_registration(
            &cid,
            bad,
            PrivateManage::registration_state_hash(&cid).unwrap(),
            None
        )
        .is_err());
        assert_eq!(
            privateadmin::AdminAccounts::<Test>::get(&cid)
                .unwrap()
                .admins
                .len(),
            3
        );
        // Failure after the wallet kernel ran must roll back the complete transaction.
        pallet::InstitutionRoleNonce::<Test>::insert(&cid, u64::MAX);
        let rotate = entitybase::InstitutionGovernanceAction::RegisterAdminWalletChanges {
            added: vec![],
            removed: vec![],
            replaced: vec![adminbase::InstitutionAdminWalletChange {
                admin_account: admin(2),
                expected_wallet: admin(2),
                new_wallet: admin(92),
                expected_revision: 0,
            }],
        };
        assert_noop!(
            send_registration(
                &cid,
                rotate,
                PrivateManage::registration_state_hash(&cid).unwrap(),
                None
            ),
            Error::<Test>::RoleNonceOverflow
        );
    });
}

#[test]
fn wallet_registration_cannot_implicitly_configure_roles_threshold_or_bypass_origin() {
    new_test_ext().execute_with(|| {
        let cid = fresh_registration_target("wallet-reg-guards");
        let hash = PrivateManage::registration_state_hash(&cid).unwrap();
        assert_noop!(
            send_registration(&cid, initial_registration_action(), hash, Some(2)),
            Error::<Test>::InvalidAssignmentResultAdmins
        );
        let action = entitybase::InstitutionGovernanceAction::RegisterAdminWalletChanges {
            added: registration_people(3),
            removed: vec![],
            replaced: vec![],
        };
        assert_noop!(
            send_registration(&cid, action.clone(), hash, Some(2)),
            Error::<Test>::InvalidRegistrationAction
        );
        assert_noop!(
            PrivateManage::register_institution_admins(
                RuntimeOrigin::signed(admin(255)),
                cid.clone(),
                action.clone(),
                b"REGISTRY-CID".to_vec(),
                b"REGISTRY-ROLE".to_vec(),
                hash,
                None
            ),
            Error::<Test>::RegistryAuthorityDenied
        );
        assert_ok!(send_registration(&cid, action, hash, None));
        let mut config = initial_registration_action();
        if let entitybase::InstitutionGovernanceAction::MutateRolesAndAssignments {
            ref mut role_mutations,
            ..
        } = config
        {
            if let entitybase::InstitutionRoleMutation::Create {
                ref mut assignments,
                ..
            } = role_mutations[0]
            {
                assignments[0].assignment_source =
                    entitybase::InstitutionAssignmentSource::InstitutionGovernance;
            }
        }
        assert_noop!(
            send_registration(
                &cid,
                config,
                PrivateManage::registration_state_hash(&cid).unwrap(),
                Some(2)
            ),
            Error::<Test>::InvalidAssignmentSource
        );
    });
}

#[test]
fn registration_call_nine_scale_fixture_covers_action_snapshot_and_initial_threshold() {
    use codec::{Decode, Encode};
    let cid = generated_cid("register-codec", "SFLP");
    let action = initial_registration_action();
    let call = crate::Call::<Test>::register_institution_admins {
        cid_number: cid.clone(),
        action: action.clone(),
        actor_cid_number: b"GD001-FRG00-000000001-2026".to_vec(),
        actor_role_code: b"REGISTRY-ROLE".to_vec(),
        expected_state_hash: [7; 32],
        initial_threshold: Some(2),
    };
    let expected = (
        9u8,
        cid.clone(),
        action,
        b"GD001-FRG00-000000001-2026".to_vec(),
        b"REGISTRY-ROLE".to_vec(),
        [7u8; 32],
        Some(2u32),
    )
        .encode();
    assert_eq!(call.encode(), expected);
    let decoded = crate::Call::<Test>::decode(&mut expected.as_slice()).unwrap();
    assert_eq!(decoded.encode(), expected);
    let old = (
        9u8,
        cid,
        registration_people(3),
        b"GD001-FRG00-000000001-2026".to_vec(),
        b"REGISTRY-ROLE".to_vec(),
    )
        .encode();
    assert!(crate::Call::<Test>::decode(&mut old.as_slice()).is_err());
}

fn wallet_proof(
    cid: &pallet::CidNumberOf<Test>,
    change: &adminbase::InstitutionAdminWalletChange<AccountId32>,
    mode: adminbase::InstitutionWalletChangeMode,
    expiry: u64,
    pair: &sr25519::Pair,
) -> adminbase::InstitutionWalletProof {
    let payload = adminbase::InstitutionWalletAcceptance {
        genesis_hash: System::block_hash(0),
        cid_number: cid.to_vec().try_into().unwrap(),
        change: change.clone(),
        mode,
        expires_at: expiry,
    }
    .encode();
    pair.sign(&primitives::sign::signing_message(
        primitives::sign::OP_SIGN_INSTITUTION_WALLET,
        &payload,
    ))
    .0
    .to_vec()
    .try_into()
    .unwrap()
}

#[test]
fn institution_self_wallet_rotation_checks_both_parties_scope_and_replay_without_mutating_identity_or_roles(
) {
    new_test_ext().execute_with(|| {
        use adminbase::{InstitutionAdminQuery, InstitutionAdminWalletQuery};
        let cid = generated_cid("wallet-self", "SFGY");
        assert_ok!(create_institution(
            cid.clone(),
            code_bytes("SFGY"),
            initial_accounts(&[])
        ));
        let code = pallet::Institutions::<Test>::get(&cid)
            .unwrap()
            .institution_code;
        let pair = sr25519::Pair::from_seed(&[91; 32]);
        let new_wallet = AccountId32::new(pair.public().0);
        let change = adminbase::InstitutionAdminWalletChange {
            admin_account: admin(1),
            expected_wallet: admin(1),
            new_wallet: new_wallet.clone(),
            expected_revision: 0,
        };
        let expiry = TestTimeProvider::now().as_secs() + 180;
        let proof = wallet_proof(
            &cid,
            &change,
            adminbase::InstitutionWalletChangeMode::SelfSigned,
            expiry,
            &pair,
        );
        let mut roster = privateadmin::AdminAccounts::<Test>::get(&cid).unwrap();
        roster
            .admins
            .iter_mut()
            .find(|a| a.account_id == change.expected_wallet)
            .unwrap()
            .account_id = new_wallet.clone();
        let assigned_role = grant_close_role(&cid);
        let role_codes_before =
            <PrivateManage as entitybase::InstitutionRoleQuery<AccountId32>>::active_role_codes(
                &cid,
                &admin(1),
            );
        let roles = pallet::InstitutionRoleAssignments::<Test>::iter_prefix(&cid)
            .collect::<Vec<_>>()
            .encode();
        let nonce = pallet::InstitutionRoleNonce::<Test>::get(&cid);
        assert_noop!(
            PrivateManage::rotate_institution_admin_wallet(
                RuntimeOrigin::signed(admin(9)),
                cid.clone(),
                change.clone(),
                expiry,
                proof.clone()
            ),
            Error::<Test>::PermissionDenied
        );
        let mut stale = change.clone();
        stale.expected_revision = 1;
        assert_noop!(
            PrivateManage::rotate_institution_admin_wallet(
                RuntimeOrigin::signed(admin(1)),
                cid.clone(),
                stale,
                expiry,
                proof.clone()
            ),
            Error::<Test>::InvalidWalletProof
        );
        let governance_proof = wallet_proof(
            &cid,
            &change,
            adminbase::InstitutionWalletChangeMode::Governance,
            expiry,
            &pair,
        );
        assert_noop!(
            PrivateManage::rotate_institution_admin_wallet(
                RuntimeOrigin::signed(admin(1)),
                cid.clone(),
                change.clone(),
                expiry,
                governance_proof
            ),
            Error::<Test>::InvalidWalletProof
        );
        assert_noop!(
            PrivateManage::rotate_institution_admin_wallet(
                RuntimeOrigin::signed(admin(1)),
                cid.clone(),
                change.clone(),
                expiry + 1,
                proof.clone()
            ),
            Error::<Test>::InvalidWalletProof
        );
        assert_ok!(PrivateManage::rotate_institution_admin_wallet(
            RuntimeOrigin::signed(admin(1)),
            cid.clone(),
            change.clone(),
            expiry,
            proof.clone()
        ));
        assert_eq!(
            privateadmin::Pallet::<Test>::resolve_admin_account(code, &cid, &new_wallet),
            Some(new_wallet.clone())
        );
        assert_eq!(
            privateadmin::Pallet::<Test>::resolve_admin_account(code, &cid, &admin(1)),
            None
        );
        assert_eq!(
            <PrivateManage as entitybase::InstitutionRoleQuery<AccountId32>>::active_role_codes(
                &cid,
                &new_wallet
            ),
            role_codes_before
        );
        assert!(<PrivateManage as entitybase::InstitutionRoleQuery<
            AccountId32,
        >>::is_active_assignment(
            &cid, &new_wallet, &assigned_role
        ));
        assert!(!<PrivateManage as entitybase::InstitutionRoleQuery<
            AccountId32,
        >>::is_active_assignment(
            &cid, &admin(1), &assigned_role
        ));
        assert_eq!(
            privateadmin::Pallet::<Test>::institution_wallet_state(code, &cid, &admin(1))
                .unwrap()
                .revision,
            1
        );
        assert_eq!(
            privateadmin::AdminAccounts::<Test>::get(&cid).unwrap(),
            roster
        );
        assert_eq!(
            pallet::InstitutionRoleAssignments::<Test>::iter_prefix(&cid)
                .collect::<Vec<_>>()
                .encode(),
            roles
        );
        assert_eq!(pallet::InstitutionRoleNonce::<Test>::get(&cid), nonce + 1);
        assert_noop!(
            PrivateManage::rotate_institution_admin_wallet(
                RuntimeOrigin::signed(admin(1)),
                cid,
                change,
                expiry,
                proof
            ),
            Error::<Test>::PermissionDenied
        );
    });
}

#[test]
fn governance_wallet_rotation_requires_existing_role_vote_and_executes_the_same_kernel() {
    new_test_ext().execute_with(|| {
        use adminbase::InstitutionAdminWalletQuery;
        let cid = generated_cid("wallet-vote", "SFGY");
        assert_ok!(create_institution(
            cid.clone(),
            code_bytes("SFGY"),
            initial_accounts(&[])
        ));
        let role = grant_close_role(&cid);
        pallet::InstitutionRolePermissions::<Test>::mutate(&cid, &role, |permissions| {
            for p in permissions.iter_mut() {
                p.business_action_id.action_code = 3;
            }
        });
        let code = pallet::Institutions::<Test>::get(&cid)
            .unwrap()
            .institution_code;
        let pair = sr25519::Pair::from_seed(&[92; 32]);
        let change = adminbase::InstitutionAdminWalletChange {
            admin_account: admin(2),
            expected_wallet: admin(2),
            new_wallet: AccountId32::new(pair.public().0),
            expected_revision: 0,
        };
        let expiry = TestTimeProvider::now().as_secs() + 86400;
        let proof = wallet_proof(
            &cid,
            &change,
            adminbase::InstitutionWalletChangeMode::Governance,
            expiry,
            &pair,
        );
        let action = entitybase::InstitutionGovernanceAction::RotateAdminWallet {
            change: change.clone(),
            expires_at: expiry,
            new_wallet_proof: proof,
        };
        assert_noop!(
            PrivateManage::propose_institution_governance(
                RuntimeOrigin::signed(admin(9)),
                cid.clone(),
                action.clone(),
                cid.to_vec(),
                role.clone()
            ),
            Error::<Test>::PermissionDenied
        );
        assert_ok!(PrivateManage::propose_institution_governance(
            RuntimeOrigin::signed(admin(1)),
            cid.clone(),
            action.clone(),
            cid.to_vec(),
            role.clone()
        ));
        let pid = VotingEngine::next_proposal_id() - 1;
        assert_eq!(
            privateadmin::Pallet::<Test>::current_institution_wallet(
                code,
                &cid,
                &change.admin_account
            ),
            Some(change.expected_wallet.clone())
        );
        assert_ok!(cast_yes_votes(pid));
        assert_eq!(
            privateadmin::Pallet::<Test>::current_institution_wallet(
                code,
                &cid,
                &change.new_wallet
            ),
            Some(change.new_wallet.clone())
        );
        assert_noop!(
            PrivateManage::propose_institution_governance(
                RuntimeOrigin::signed(admin(1)),
                cid.clone(),
                action,
                cid.to_vec(),
                role
            ),
            Error::<Test>::InvalidWalletProof
        );
    });
}

#[test]
fn registry_wallet_unincorporated_institution_uses_its_owner_pallet() {
    new_test_ext().execute_with(|| {
        let old = fresh_registration_target("registry-wallet-unin-parent");
        let cid = generated_cid("registry-wallet-unin", "UNIN");
        let mut institution = pallet::Institutions::<Test>::take(&old).unwrap();
        institution.institution_code = *b"UNIN";
        pallet::Institutions::<Test>::insert(&cid, institution);
        let action = entitybase::InstitutionGovernanceAction::RegisterAdminWalletChanges {
            added: registration_people(1),
            removed: vec![],
            replaced: vec![],
        };
        assert_ok!(send_registration(
            &cid,
            action,
            PrivateManage::registration_state_hash(&cid).unwrap(),
            None
        ));
        assert_eq!(
            privateadmin::AdminAccounts::<Test>::get(&cid)
                .unwrap()
                .institution_code,
            *b"UNIN"
        );
        assert!(
            pallet::InstitutionRoleAssignments::<Test>::iter_prefix(&cid)
                .next()
                .is_none()
        );
    });
}

#[test]
fn retired_roster_actions_cannot_create_or_execute_and_original_votes_remain() {
    use codec::Encode;
    use engingcore::InternalVoteEngine;
    new_test_ext().execute_with(||{
        let cid=generated_cid("roster-stale", "SFGY");
        assert_ok!(create_institution(cid.clone(),code_bytes("SFGY"),initial_accounts(&[(crate::RESERVED_NAME_MAIN,0),(crate::RESERVED_NAME_FEE,0)])));
        let role=grant_close_role(&cid);
        pallet::InstitutionRolePermissions::<Test>::mutate(&cid,&role,|items|{for p in items {p.business_action_id.action_code=3;}});
        let rows=privateadmin::AdminAccounts::<Test>::get(&cid).unwrap().admins.to_vec();
        let mut added=rows.clone();let mut unproved=rows[0].clone();unproved.account_id=admin(99);unproved.cid_number=Default::default();added.push(unproved);
        let actions=vec![entitybase::InstitutionGovernanceAction::ReplaceAdmins{admins:added.clone()},entitybase::InstitutionGovernanceAction::ReplaceAdminsAndMutateRoles{admins:added,role_mutations:vec![entitybase::InstitutionRoleMutation::Rename{role_code:role.to_vec(),role_name:b"must-not-apply".to_vec()}],assignment_changes:vec![],legal_representative_change:None}];
        for action in actions {
            let roster_before=privateadmin::AdminAccounts::<Test>::get(&cid).encode();
            let personnel_before=pallet::InstitutionRoleAssignments::<Test>::iter_prefix(&cid).collect::<Vec<_>>().encode();
            assert_noop!(PrivateManage::propose_institution_governance(RuntimeOrigin::signed(admin(1)),cid.clone(),action.clone(),cid.to_vec(),role.clone()),Error::<Test>::InvalidInstitutionGovernanceAction);
            assert_noop!(send_registration(&cid,action.clone(),PrivateManage::registration_state_hash(&cid).unwrap(),None),Error::<Test>::InvalidRegistrationAction);
            // 仅在隔离测试中复现已在链上的旧提案；生产入口绝不重新接纳旧动作。
            let stored=entitybase::InstitutionGovernanceProposal{institution_code:pallet::Institutions::<Test>::get(&cid).unwrap().institution_code,cid_number:cid.to_vec(),action};
            let mut data=crate::MODULE_TAG.to_vec();data.push(crate::pallet::ACTION_GOVERNANCE);data.extend(stored.encode());
            let plan=PrivateManage::build_institution_vote_plan(&admin(1),&cid,&role,3,&data).unwrap();
            let subject=plan.proposer_subject.clone();
            let pid=<InternalVote as InternalVoteEngine<AccountId32>>::create_institution_admin_change_proposal_with_data(admin(1),stored.institution_code,cid.to_vec(),plan,data).unwrap();
            let snapshot=engingcore::VoterSnapshot::<Test>::get(pid,&subject).unwrap();
            let versions=engingcore::FrozenWalletVersions::<Test>::get(pid,&subject).unwrap();
            for voter in snapshot.iter().filter(|v| **v!=admin(1)) {
                if engingcore::Proposals::<Test>::get(pid).unwrap().status==engingcore::STATUS_VOTING {
                    assert_ok!(InternalVote::cast(RuntimeOrigin::signed(voter.clone()),pid,internalvote::InternalVoteTicketClaim::InstitutionRole(role.clone()),true));
                }
            }
            let tally=internalvote::InternalTallies::<Test>::get(pid);
            let tickets=internalvote::InternalVotesByTicket::<Test>::iter_prefix(pid).collect::<Vec<_>>().encode();
            <VotingEngine as frame_support::traits::Hooks<u64>>::on_initialize(System::block_number());
            assert_eq!(engingcore::Proposals::<Test>::get(pid).unwrap().status,engingcore::STATUS_EXECUTION_FAILED);
            assert_eq!(engingcore::VoterSnapshot::<Test>::get(pid,&subject),Some(snapshot));
            assert_eq!(engingcore::FrozenWalletVersions::<Test>::get(pid,&subject),Some(versions));
            assert_eq!(internalvote::InternalTallies::<Test>::get(pid),tally);
            assert_eq!(internalvote::InternalVotesByTicket::<Test>::iter_prefix(pid).collect::<Vec<_>>().encode(),tickets);
            assert_eq!(privateadmin::AdminAccounts::<Test>::get(&cid).encode(),roster_before);
            assert_eq!(pallet::InstitutionRoleAssignments::<Test>::iter_prefix(&cid).collect::<Vec<_>>().encode(),personnel_before);
            System::set_block_number(System::block_number()+1);
        }
    });
}

/// System存活引用是本步允许的唯一账户新增状态；nonce、providers、余额等仍须完全一致。
fn account_state_without_wallet_refs() -> Vec<(
    AccountId32,
    frame_system::AccountInfo<
        <Test as frame_system::Config>::Nonce,
        <Test as frame_system::Config>::AccountData,
    >,
)> {
    frame_system::Account::<Test>::iter()
        .filter_map(|(wallet, mut info)| {
            info.sufficients = 0;
            (info != Default::default()).then_some((wallet, info))
        })
        .collect()
}

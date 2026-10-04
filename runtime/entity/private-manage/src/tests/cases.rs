use super::*;
use frame_support::traits::GetCallName;
use frame_support::{assert_noop, assert_ok};
use votingengine::{types::code_bytes, InternalVoteEngine as _};

#[test]
fn direct_institution_creation_call_is_permanently_absent() {
    let calls = <pallet::Call<Test> as GetCallName>::get_call_names();
    assert!(!calls.contains(&"propose_create_private_institution"));
}

fn governance_assignment(
    account_id: AccountId32,
) -> entity_primitives::InstitutionAssignmentTarget<AccountId32> {
    entity_primitives::InstitutionAssignmentTarget {
        account_id,
        term_start: 0,
        term_end: 0,
        assignment_source: entity_primitives::InstitutionAssignmentSource::InstitutionGovernance,
        assignment_source_ref: b"proposal-result".to_vec(),
        assignment_status: entity_primitives::InstitutionAssignmentStatus::Active,
    }
}

#[test]
fn private_dynamic_role_is_authorized_only_for_assigned_admin() {
    new_test_ext().execute_with(|| {
        use entity_primitives::{
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
        let action = entity_primitives::BusinessActionId {
            module_tag: b"pri-mgmt".to_vec(),
            action_code: 3,
        };
        assert_ok!(PrivateManage::apply_institution_governance_result(
            entity_primitives::InstitutionGovernanceResult {
                institution_code: code,
                cid_number: cid_number.to_vec(),
                proposal_id: 51,
                role_mutations: vec![InstitutionRoleMutation::Create {
                    role_name: "财务负责人".as_bytes().to_vec(),
                    term_required: false,
                    permissions: vec![entity_primitives::RolePermissionSpec {
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

        let role_code = entity_primitives::generate_dynamic_role_code(
            crate::MODULE_TAG,
            cid_number.as_slice(),
            0,
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
fn operation_phase_lr_without_roster_cid_falls_back_to_identity_record() {
    new_test_ext().execute_with(|| {
        use entity_primitives::{
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
            entity_primitives::InstitutionGovernanceResult {
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
        let action = entity_primitives::BusinessActionId {
            module_tag: b"pri-mgmt".to_vec(),
            action_code: 3,
        };
        assert_ok!(PrivateManage::apply_institution_governance_result(
            entity_primitives::InstitutionGovernanceResult {
                institution_code: code,
                cid_number: cid_number.to_vec(),
                proposal_id: 81,
                role_mutations: vec![InstitutionRoleMutation::Create {
                    role_name: "业务负责人".as_bytes().to_vec(),
                    term_required: false,
                    permissions: vec![entity_primitives::RolePermissionSpec {
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
        let role_code = entity_primitives::generate_dynamic_role_code(
            crate::MODULE_TAG,
            cid_number.as_slice(),
            0,
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

        // 运行期 + LR 换绑到新钱包：名册 cid 为空 → 回落到 LR 身份记录的 cid 完成解析。
        set_operation_phase(true);
        bind_cid(&citizen_cid, admin(5));
        assert!(authorized(&admin(5))); // LR 换绑不掉权（缺口已闭合）
        assert!(!authorized(&admin(1))); // LR 旧钱包掉权
        assert!(authorized(&admin(2))); // 非 LR 且无 cid → 仍按 account_id（分层规则不被破坏）
    });
}

#[test]
fn operation_phase_authorizes_by_cid_and_survives_wallet_rebind() {
    new_test_ext().execute_with(|| {
        use entity_primitives::{
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

        // 用带 CID 的名册覆盖：admin(1) 携 CID、admin(2) 无 CID（private-admins 无 seed 期绑定校验）。
        let citizen_cid = b"CN220-CTZN2-198805200-2026".to_vec();
        let admins: crate::InstitutionAdminsInputOf<Test> = vec![
            admin_primitives::Admin {
                account_id: admin(1),
                cid_number: citizen_cid.clone().try_into().expect("cid fits"),
                family_name: "张".as_bytes().to_vec().try_into().expect("family fits"),
                given_name: "三".as_bytes().to_vec().try_into().expect("given fits"),
            },
            admin_primitives::Admin {
                account_id: admin(2),
                cid_number: Default::default(),
                family_name: "管理".as_bytes().to_vec().try_into().expect("family fits"),
                given_name: "员".as_bytes().to_vec().try_into().expect("given fits"),
            },
        ]
        .try_into()
        .expect("admins fit");
        assert_ok!(PrivateManage::set_institution_admins(
            &cid_number,
            code,
            &admins
        ));

        let action = entity_primitives::BusinessActionId {
            module_tag: b"pri-mgmt".to_vec(),
            action_code: 3,
        };
        assert_ok!(PrivateManage::apply_institution_governance_result(
            entity_primitives::InstitutionGovernanceResult {
                institution_code: code,
                cid_number: cid_number.to_vec(),
                proposal_id: 70,
                role_mutations: vec![InstitutionRoleMutation::Create {
                    role_name: "业务负责人".as_bytes().to_vec(),
                    term_required: false,
                    permissions: vec![entity_primitives::RolePermissionSpec {
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
        let role_code = entity_primitives::generate_dynamic_role_code(
            crate::MODULE_TAG,
            cid_number.as_slice(),
            0,
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

        // 运行期：admin(1) 换绑到新钱包 admin(5)。
        set_operation_phase(true);
        bind_cid(&citizen_cid, admin(5));
        assert!(authorized(&admin(5))); // 换绑不掉权
        assert!(!authorized(&admin(1))); // 旧钱包掉权
        assert!(authorized(&admin(2))); // 无 CID 管理员运行期仍按 account_id
    });
}

#[test]
fn private_legal_representative_role_is_unique_and_allows_zero_or_one_assignment() {
    new_test_ext().execute_with(|| {
        use entity_primitives::{
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
                entity_primitives::InstitutionGovernanceResult {
                    institution_code: code,
                    cid_number: cid_number.to_vec(),
                    proposal_id: 52,
                    role_mutations: vec![InstitutionRoleMutation::Create {
                        role_name:
                            primitives::institution_constraints::ROLE_NAME_LEGAL_REPRESENTATIVE
                                .to_vec(),
                        term_required: false,
                        permissions: vec![entity_primitives::RolePermissionSpec {
                            business_action_id: entity_primitives::BusinessActionId {
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
            entity_primitives::InstitutionGovernanceResult {
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
            entity_primitives::InstitutionGovernanceResult {
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
        use entity_primitives::{
            InstitutionLegalRepresentativeChange, InstitutionRoleAssignmentChange,
        };
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
            entity_primitives::InstitutionGovernanceResult {
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
        use entity_primitives::{
            InstitutionLegalRepresentativeChange, InstitutionRoleAssignmentChange,
        };
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
                entity_primitives::InstitutionGovernanceResult {
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
                entity_primitives::InstitutionGovernanceResult {
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
            entity_primitives::InstitutionGovernanceResult {
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
fn rejected_add_is_cleaned_only_by_votingengine_callback() {
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
            <crate::InternalVoteExecutor<Test> as votingengine::InternalVoteResultCallback>::on_internal_vote_finalized(
                proposal_id,
                false,
            ),
            Ok(votingengine::ProposalExecutionOutcome::Executed)
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
fn rejected_close_is_cleaned_only_by_votingengine_callback() {
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
            <crate::InternalVoteExecutor<Test> as votingengine::InternalVoteResultCallback>::on_internal_vote_finalized(
                proposal_id,
                false,
            ),
            Ok(votingengine::ProposalExecutionOutcome::Executed)
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

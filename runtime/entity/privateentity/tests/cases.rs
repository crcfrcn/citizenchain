//! 验证私权机构账户、岗位和治理登记的授权及状态变化。

use super::*;
use engingcore::{types::code_bytes, InternalVoteEngine as _};
use frame_support::traits::GetCallName;
use frame_support::{assert_noop, assert_ok};

#[test]
fn direct_institution_creation_call_is_permanently_absent() {
    let calls = <pallet::Call<Test> as GetCallName>::get_call_names();
    assert!(!calls.contains(&"propose_create_private_institution"));
}

fn governance_assignment(
    account_id: AccountId32,
) -> entitybase::InstitutionAssignmentTarget {
    entitybase::InstitutionAssignmentTarget {
        person_cid: fixture_person(&account_id, "CTZN").to_vec(),
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
        assert_ok!(PrivateEntity::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: code,
                cid_number: cid_number.to_vec(),
                proposal_id: 51,
                role_mutations: vec![InstitutionRoleMutation::Create {
                    role_name: "财务负责人".as_bytes().to_vec(),
                    term_required: false, seats: 3,
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
        assert!(<PrivateEntity as InstitutionRoleAuthorizationQuery<
            AccountId32,
        >>::is_authorized(
            &admin(1),
            &subject,
            &action,
            RolePermissionOperation::Propose,
        ));
        assert!(!<PrivateEntity as InstitutionRoleAuthorizationQuery<
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
            PrivateEntity::apply_institution_governance_result(
                entitybase::InstitutionGovernanceResult {
                    institution_code: code,
                    cid_number: cid_number.to_vec(),
                    proposal_id: 52,
                    role_mutations: vec![InstitutionRoleMutation::Create {
                        role_name:
                            primitives::institution_constraints::ROLE_NAME_LEGAL_REPRESENTATIVE
                                .to_vec(),
                        term_required: false, seats: 3,
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
        assert_ok!(PrivateEntity::apply_institution_governance_result(
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
                    person_cid: fixture_person(&admin(1), "CTZN").to_vec(),
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

        assert_ok!(PrivateEntity::apply_institution_governance_result(
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
            PrivateEntity::apply_institution_governance_result(
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
                        person_cid: fixture_person(&admin(1), "CTZN").to_vec(),
                    }),
                    result_source_ref: b"proposal-61".to_vec(),
                }
            ),
            Error::<Test>::EmptyLegalRepresentativeName
        );

        // 运行期:空 CID → 拒。
        assert_noop!(
            PrivateEntity::apply_institution_governance_result(
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
                        person_cid: vec![],
                    }),
                    result_source_ref: b"proposal-62".to_vec(),
                }
            ),
            Error::<Test>::InvalidAssignmentPerson
        );

        // 运行期:四要素齐全 → Ok。
        assert_ok!(PrivateEntity::apply_institution_governance_result(
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
                    person_cid: fixture_person(&admin(1), "CTZN").to_vec(),
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

        assert_ok!(PrivateEntity::update_institution_info(
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

        assert_ok!(PrivateEntity::propose_close_private_institution(
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

        assert_ok!(PrivateEntity::propose_close_private_institution(
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
            PrivateEntity::propose_close_private_institution(
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
            PrivateEntity::propose_close_private_institution(
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
            PrivateEntity::propose_close_private_institution(
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

#[test]
fn private_role_accepts_resident_cid() {
    use entitybase::{InstitutionAssignmentTarget, InstitutionRoleAssignmentChange};
    new_test_ext().execute_with(|| {
        let cid = generated_cid("private-resident", "SFLP");
        assert_ok!(create_institution(cid.clone(), *b"SFLP", initial_accounts(&[])));
        let wallet = admin(9);
        let person_cid = fixture_person(&wallet, "NATP");
        let role_code = b"TEST_CLOSE_ROLE".to_vec();
        assert_ok!(PrivateEntity::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: *b"SFLP",
                cid_number: cid.to_vec(),
                proposal_id: 90,
                role_mutations: vec![],
                assignment_changes: vec![InstitutionRoleAssignmentChange {
                    role_code: role_code.clone(),
                    assignments: vec![
                        InstitutionAssignmentTarget {
                            person_cid: person_cid.to_vec(),
                            term_start: 0,
                            term_end: 0,
                            assignment_source: InstitutionAssignmentSource::InstitutionGovernance,
                            assignment_source_ref: b"fixture".to_vec(),
                            assignment_status: InstitutionAssignmentStatus::Active,
                        },
                        governance_assignment(admin(2))
                    ],
                }],
                legal_representative_change: None,
                result_source_ref: b"fixture".to_vec(),
            }
        ));
        assert!(<PrivateEntity as entitybase::InstitutionRoleQuery<
            AccountId32,
        >>::is_active_assignment(&cid, &wallet, &role_code));
    });
}

// 原生政策回归：真实建案、原阈值投票、回调执行和失败终态。
fn policy_fixture(tag: &str) -> (pallet::CidNumberOf<Test>, crate::RoleCodeOf) {
    let cid = generated_cid(tag, "SFLP");
    assert_ok!(create_institution(
        cid.clone(),
        code_bytes("SFLP"),
        initial_accounts(&[
            (crate::RESERVED_NAME_MAIN, 0),
            (crate::RESERVED_NAME_FEE, 0)
        ])
    ));
    let nonce = pallet::InstitutionRoleNonce::<Test>::get(&cid);
    let raw = entitybase::generate_dynamic_role_code(crate::MODULE_TAG, &cid, nonce, 900);
    let role: crate::RoleCodeOf = raw.clone().try_into().unwrap();
    assert_ok!(PrivateEntity::apply_institution_governance_result(
        entitybase::InstitutionGovernanceResult {
            institution_code: code_bytes("SFLP"),
            cid_number: cid.to_vec(),
            proposal_id: 900,
            role_mutations: vec![entitybase::InstitutionRoleMutation::Create {
                role_name: b"Native treasury".to_vec(),
                term_required: false,
                seats: 5,
                permissions: policy_permissions(false),
                assignments: (0..3).map(|n| policy_assignment(admin(n))).collect(),
            }],
            assignment_changes: vec![],
            legal_representative_change: None,
            result_source_ref: b"proposal-900".to_vec(),
        }
    ));
    (cid, role)
}

fn policy_assignment(wallet: AccountId32) -> entitybase::InstitutionAssignmentTarget {
    entitybase::InstitutionAssignmentTarget {
        person_cid: fixture_person(&wallet, "CTZN").to_vec(),
        term_start: 0,
        term_end: 0,
        assignment_source: entitybase::InstitutionAssignmentSource::InstitutionGovernance,
        assignment_source_ref: b"policy-result".to_vec(),
        assignment_status: entitybase::InstitutionAssignmentStatus::Active,
    }
}

fn policy_permissions(propose_governance: bool) -> Vec<entitybase::RolePermissionSpec> {
    let mut permissions = [
        entitybase::RolePermissionOperation::Propose,
        entitybase::RolePermissionOperation::Vote,
    ]
    .into_iter()
    .map(|operation| entitybase::RolePermissionSpec {
        business_action_id: entitybase::BusinessActionId {
            module_tag: entitybase::business_action::MODULE_MULTISIG.to_vec(),
            action_code: 0,
        },
        operation,
    })
    .collect::<Vec<_>>();
    if propose_governance {
        permissions.push(entitybase::RolePermissionSpec {
            business_action_id: entitybase::BusinessActionId {
                module_tag: crate::MODULE_TAG.to_vec(),
                action_code: 3,
            },
            operation: entitybase::RolePermissionOperation::Propose,
        });
    }
    permissions
}

fn policy_action(role: &crate::RoleCodeOf) -> entitybase::InstitutionPolicyAction {
    entitybase::InstitutionPolicyAction {
        permission_changes: vec![entitybase::RolePermissionChange {
            role_code: role.to_vec(),
            permissions: policy_permissions(true),
        }],
        threshold: None,
    }
}

fn policy_proposer() -> crate::RoleCodeOf {
    b"TEST_CLOSE_ROLE".to_vec().try_into().unwrap()
}
fn policy_cast(wallet: AccountId32, id: u128) -> sp_runtime::DispatchResult {
    InternalVote::cast(
        RuntimeOrigin::signed(wallet),
        id,
        internalvote::InternalVoteTicketClaim::InstitutionRole(policy_proposer()),
        true,
    )
}
fn policy_execute() {
    <VotingEngine as frame_support::traits::Hooks<u64>>::on_initialize(System::block_number());
}

#[test]
fn policy_rejects_empty_permissions_bad_threshold_stale_state_and_non_actor_atomically() {
    new_test_ext().execute_with(|| {
        let (cid, role) = policy_fixture("policy-errors");
        let state = PrivateEntity::registration_state_hash(&cid).unwrap();
        assert_noop!(
            PrivateEntity::propose_institution_policy(
                RuntimeOrigin::signed(admin(9)),
                cid.clone(),
                policy_action(&role),
                policy_proposer(),
                state
            ),
            Error::<Test>::PermissionDenied
        );
        assert_noop!(
            PrivateEntity::propose_institution_policy(
                RuntimeOrigin::signed(admin(1)),
                cid.clone(),
                policy_action(&role),
                policy_proposer(),
                [0; 32]
            ),
            Error::<Test>::StalePolicyState
        );
        let mut empty = policy_action(&role);
        empty.permission_changes[0].permissions.clear();
        assert_noop!(
            PrivateEntity::propose_institution_policy(
                RuntimeOrigin::signed(admin(1)),
                cid.clone(),
                empty,
                policy_proposer(),
                state
            ),
            Error::<Test>::RolePermissionsEmpty
        );
        let mut duplicate = policy_action(&role);
        let permission = duplicate.permission_changes[0].permissions[0].clone();
        duplicate.permission_changes[0].permissions.push(permission);
        assert_noop!(
            PrivateEntity::propose_institution_policy(
                RuntimeOrigin::signed(admin(1)),
                cid.clone(),
                duplicate,
                policy_proposer(),
                state
            ),
            Error::<Test>::DuplicateRolePermission
        );
        for threshold in [0, 1, 4] {
            let mut action = policy_action(&role);
            action.threshold = Some(threshold);
            assert_noop!(
                PrivateEntity::propose_institution_policy(
                    RuntimeOrigin::signed(admin(1)),
                    cid.clone(),
                    action,
                    policy_proposer(),
                    state
                ),
                Error::<Test>::InvalidThreshold
            );
        }
        let mut fixed = policy_action(&role);
        fixed.permission_changes[0].role_code = b"LR".to_vec();
        assert_noop!(
            PrivateEntity::propose_institution_policy(
                RuntimeOrigin::signed(admin(1)),
                cid.clone(),
                fixed,
                policy_proposer(),
                state
            ),
            Error::<Test>::InstitutionPolicyDenied
        );
        assert_eq!(PrivateEntity::registration_state_hash(&cid).unwrap(), state);
        assert_eq!(VotingEngine::next_proposal_id(), 0);
    });
}

#[test]
fn policy_uses_old_people_and_threshold_then_executes_after_passed() {
    new_test_ext().execute_with(|| {
        let (cid, role) = policy_fixture("policy-execute");
        let state = PrivateEntity::registration_state_hash(&cid).unwrap();
        assert_ok!(PrivateEntity::propose_institution_policy(
            RuntimeOrigin::signed(admin(1)),
            cid.clone(),
            policy_action(&role),
            policy_proposer(),
            state
        ));
        let id = VotingEngine::next_proposal_id() - 1;
        assert_eq!(
            internalvote::InternalThresholdSnapshot::<Test>::get(id),
            Some(2)
        );
        assert_eq!(
            pallet::InstitutionRolePermissions::<Test>::get(&cid, &role).len(),
            2
        );
        let old = admin(2);
        let current = admin(8);
        let person = fixture_person(&old, "CTZN");
        PEOPLE.with(|people| {
            people.borrow_mut().insert(person, current.clone());
        });
        assert!(policy_cast(old, id).is_err());
        assert_ok!(policy_cast(current.clone(), id));
        assert!(policy_cast(current, id).is_err());
        assert_eq!(
            VotingEngine::proposals(id).unwrap().status,
            engingcore::STATUS_PASSED
        );
        assert_eq!(
            pallet::InstitutionRolePermissions::<Test>::get(&cid, &role).len(),
            2
        );
        policy_execute();
        assert_eq!(
            VotingEngine::proposals(id).unwrap().status,
            engingcore::STATUS_EXECUTED
        );
        assert_eq!(
            pallet::InstitutionRolePermissions::<Test>::get(&cid, &role).len(),
            3
        );
        assert!(pallet::InstitutionRoles::<Test>::contains_key(&cid, &role));
        assert_eq!(
            pallet::InstitutionRoleAssignments::<Test>::get(&cid, &role).len(),
            3
        );
    });
}

#[test]
fn stale_policy_is_fatal_and_uses_existing_terminal_cleanup() {
    new_test_ext().execute_with(|| {
        let (cid, role) = policy_fixture("policy-stale");
        let state = PrivateEntity::registration_state_hash(&cid).unwrap();
        assert_ok!(PrivateEntity::propose_institution_policy(
            RuntimeOrigin::signed(admin(1)),
            cid.clone(),
            policy_action(&role),
            policy_proposer(),
            state
        ));
        let id = VotingEngine::next_proposal_id() - 1;
        assert_ok!(PrivateEntity::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: code_bytes("SFLP"),
                cid_number: cid.to_vec(),
                proposal_id: 901,
                role_mutations: vec![entitybase::InstitutionRoleMutation::Rename {
                    role_code: role.to_vec(),
                    role_name: b"Renamed treasury".to_vec()
                }],
                assignment_changes: vec![entitybase::InstitutionRoleAssignmentChange {
                    role_code: policy_proposer().to_vec(),
                    assignments: vec![policy_assignment(admin(3)), policy_assignment(admin(4))]
                }],
                legal_representative_change: None,
                result_source_ref: b"proposal-901".to_vec(),
            }
        ));
        assert_ok!(policy_cast(admin(2), id));
        policy_execute();
        assert_eq!(
            VotingEngine::proposals(id).unwrap().status,
            engingcore::STATUS_EXECUTION_FAILED
        );
        assert_eq!(
            pallet::InstitutionRolePermissions::<Test>::get(&cid, &role).len(),
            2
        );
        assert!(!engingcore::ProposalExecutionRetryStates::<Test>::contains_key(id));
        assert!(!engingcore::PendingProposalExecutions::<Test>::contains_key(id));
        assert!(engingcore::ScheduledCleanupTail::<Test>::get() > 0);
    });
}

#[test]
fn configure_is_atomic_for_capacity_term_and_stop_and_delete() {
    new_test_ext().execute_with(|| {
        let (cid, role) = policy_fixture("policy-configure");
        let result = |seats, term_required, role_status, assignments| {
            entitybase::InstitutionGovernanceResult {
                institution_code: code_bytes("SFLP"),
                cid_number: cid.to_vec(),
                proposal_id: 902,
                role_mutations: vec![entitybase::InstitutionRoleMutation::Configure {
                    role_code: role.to_vec(),
                    term_required,
                    seats,
                    role_status,
                }],
                assignment_changes: assignments,
                legal_representative_change: None,
                result_source_ref: b"proposal-902".to_vec(),
            }
        };
        let state = PrivateEntity::registration_state_hash(&cid).unwrap();
        assert_noop!(
            PrivateEntity::apply_institution_governance_result(result(
                2,
                false,
                entitybase::InstitutionRoleStatus::Active,
                vec![]
            )),
            Error::<Test>::FixedRoleSeatsMismatch
        );
        assert_noop!(
            PrivateEntity::apply_institution_governance_result(result(
                5,
                true,
                entitybase::InstitutionRoleStatus::Active,
                vec![]
            )),
            Error::<Test>::InvalidAssignmentTerm
        );
        assert_eq!(PrivateEntity::registration_state_hash(&cid).unwrap(), state);
        let assignments = (0..2).map(|n| policy_assignment(admin(n))).collect();
        assert_ok!(PrivateEntity::apply_institution_governance_result(result(
            2,
            false,
            entitybase::InstitutionRoleStatus::Active,
            vec![entitybase::InstitutionRoleAssignmentChange {
                role_code: role.to_vec(),
                assignments
            }]
        )));
        assert_eq!(
            pallet::InstitutionRoles::<Test>::get(&cid, &role)
                .unwrap()
                .seats,
            2
        );
        let today = 1_782_864_000u32 / 86_400;
        let assignments = (0..2)
            .map(|n| {
                let mut assignment = policy_assignment(admin(n));
                assignment.term_start = today;
                assignment.term_end = today;
                assignment
            })
            .collect();
        assert_ok!(PrivateEntity::apply_institution_governance_result(result(
            2,
            true,
            entitybase::InstitutionRoleStatus::Active,
            vec![entitybase::InstitutionRoleAssignmentChange {
                role_code: role.to_vec(),
                assignments
            }]
        )));
        assert_eq!(PrivateEntity::snapshot_role_persons(&cid, &role).len(), 2);
        let state = PrivateEntity::registration_state_hash(&cid).unwrap();
        let assignments = (0..2)
            .map(|n| {
                let mut assignment = policy_assignment(admin(n));
                assignment.term_start = today + 1;
                assignment.term_end = today + 1;
                assignment
            })
            .collect();
        assert_noop!(
            PrivateEntity::apply_institution_governance_result(result(
                2,
                true,
                entitybase::InstitutionRoleStatus::Active,
                vec![entitybase::InstitutionRoleAssignmentChange {
                    role_code: role.to_vec(),
                    assignments
                }]
            )),
            Error::<Test>::InvalidThreshold
        );
        assert_eq!(PrivateEntity::registration_state_hash(&cid).unwrap(), state);
        assert_ok!(PrivateEntity::apply_institution_governance_result(result(
            2,
            false,
            entitybase::InstitutionRoleStatus::Inactive,
            vec![entitybase::InstitutionRoleAssignmentChange {
                role_code: role.to_vec(),
                assignments: vec![]
            }]
        )));
        assert!(PrivateEntity::snapshot_role_persons(&cid, &role).is_empty());
        assert_ok!(PrivateEntity::propose_institution_policy(
            RuntimeOrigin::signed(admin(1)),
            cid.clone(),
            entitybase::InstitutionPolicyAction {
                permission_changes: vec![],
                threshold: Some(2)
            },
            policy_proposer(),
            PrivateEntity::registration_state_hash(&cid).unwrap()
        ));
        assert_ok!(PrivateEntity::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: code_bytes("SFLP"),
                cid_number: cid.to_vec(),
                proposal_id: 903,
                role_mutations: vec![entitybase::InstitutionRoleMutation::Delete {
                    role_code: role.to_vec()
                }],
                assignment_changes: vec![],
                legal_representative_change: None,
                result_source_ref: b"proposal-903".to_vec(),
            }
        ));
        assert!(!pallet::InstitutionRoles::<Test>::contains_key(&cid, &role));
        assert!(pallet::UsedRoleCodes::<Test>::get(&cid, &role));
    });
}

#[test]
fn policy_missing_binding_cannot_reduce_denominator() {
    new_test_ext().execute_with(|| {
        let (cid, role) = policy_fixture("policy-binding");
        let person = fixture_person(&admin(0), "CTZN");
        PEOPLE.with(|people| {
            people.borrow_mut().remove(&person);
        });
        let state = PrivateEntity::registration_state_hash(&cid).unwrap();
        assert_noop!(
            PrivateEntity::propose_institution_policy(
                RuntimeOrigin::signed(admin(1)),
                cid.clone(),
                policy_action(&role),
                policy_proposer(),
                state
            ),
            Error::<Test>::InvalidAssignmentPerson
        );
        assert_eq!(PrivateEntity::registration_state_hash(&cid).unwrap(), state);
    });
}

#[test]
fn common_threshold_rejects_disjoint_electorates_without_partial_roles() {
    new_test_ext().execute_with(|| {
        let (cid, role) = policy_fixture("policy-threshold");
        let state = PrivateEntity::registration_state_hash(&cid).unwrap();
        let assignments = (0..5).map(|n| policy_assignment(admin(n))).collect();
        assert_noop!(
            PrivateEntity::apply_institution_governance_result(
                entitybase::InstitutionGovernanceResult {
                    institution_code: code_bytes("SFLP"),
                    cid_number: cid.to_vec(),
                    proposal_id: 904,
                    role_mutations: vec![],
                    assignment_changes: vec![entitybase::InstitutionRoleAssignmentChange {
                        role_code: role.to_vec(),
                        assignments
                    }],
                    legal_representative_change: None,
                    result_source_ref: b"proposal-904".to_vec(),
                }
            ),
            Error::<Test>::InvalidThreshold
        );
        assert_eq!(PrivateEntity::registration_state_hash(&cid).unwrap(), state);
    });
}

#[test]
fn first_registration_rejects_empty_chain_rights_and_rolls_back_incomplete_governance() {
    new_test_ext().execute_with(|| {
        let (cid, _) = policy_fixture("policy-registration");
        let codes = pallet::InstitutionRoles::<Test>::iter_prefix(&cid)
            .map(|(code, _)| code)
            .collect::<Vec<_>>();
        for code in codes {
            if code.as_slice() != b"LR" {
                pallet::InstitutionRoles::<Test>::remove(&cid, &code);
                pallet::InstitutionRolePermissions::<Test>::remove(&cid, &code);
                pallet::InstitutionRoleAssignments::<Test>::remove(&cid, &code);
            }
        }
        pallet::InstitutionGovernanceThresholds::<Test>::remove(&cid);
        let state = PrivateEntity::registration_state_hash(&cid).unwrap();
        let used = pallet::UsedRoleCodes::<Test>::iter_prefix(&cid).collect::<Vec<_>>();
        let action = |operations: Vec<entitybase::RolePermissionOperation>| {
            entitybase::InstitutionRoleRegistration {
                role_mutations: vec![entitybase::InstitutionRoleMutation::Create {
                    role_name: b"Initial native governance".to_vec(),
                    term_required: false,
                    seats: 2,
                    permissions: operations
                        .into_iter()
                        .map(|operation| entitybase::RolePermissionSpec {
                            business_action_id: entitybase::BusinessActionId {
                                module_tag: crate::MODULE_TAG.to_vec(),
                                action_code: 3,
                            },
                            operation,
                        })
                        .collect(),
                    assignments: [admin(1), admin(2)]
                        .into_iter()
                        .map(|wallet| {
                            let mut target = policy_assignment(wallet);
                            target.assignment_source =
                                entitybase::InstitutionAssignmentSource::Registry;
                            target.assignment_source_ref.clear();
                            target
                        })
                        .collect(),
                }],
                assignment_changes: vec![],
            }
        };
        let register = |action, hash, threshold| {
            PrivateEntity::register_institution_roles(
                RuntimeOrigin::signed(registrar()),
                cid.clone(),
                action,
                b"GD001-FRG00-000000001-2026".to_vec(),
                b"REGISTRY-ROLE".to_vec(),
                hash,
                threshold,
            )
        };
        assert_noop!(
            register(action(vec![]), state, Some(2)),
            Error::<Test>::RolePermissionsEmpty
        );
        assert_noop!(
            register(
                action(vec![entitybase::RolePermissionOperation::Propose]),
                state,
                Some(2)
            ),
            Error::<Test>::InvalidThreshold
        );
        let mut oversized = action(vec![
            entitybase::RolePermissionOperation::Propose,
            entitybase::RolePermissionOperation::Vote,
        ]);
        if let entitybase::InstitutionRoleMutation::Create {
            seats, assignments, ..
        } = &mut oversized.role_mutations[0]
        {
            *seats = 6;
            *assignments = (0..6)
                .map(|n| {
                    let mut target = policy_assignment(admin(n));
                    target.assignment_source = entitybase::InstitutionAssignmentSource::Registry;
                    target.assignment_source_ref.clear();
                    target
                })
                .collect();
        }
        let mut second = oversized.role_mutations[0].clone();
        if let entitybase::InstitutionRoleMutation::Create { role_name, .. } = &mut second {
            *role_name = b"Other initial governance".to_vec();
        }
        oversized.role_mutations.push(second);
        assert_noop!(
            register(oversized, state, Some(7)),
            Error::<Test>::TooManyInstitutionAssignments
        );
        assert_eq!(PrivateEntity::registration_state_hash(&cid).unwrap(), state);
        assert_eq!(
            pallet::UsedRoleCodes::<Test>::iter_prefix(&cid).collect::<Vec<_>>(),
            used
        );
        assert_eq!(
            pallet::InstitutionRoles::<Test>::iter_prefix(&cid).count(),
            1
        );
        assert_ok!(register(
            action(vec![
                entitybase::RolePermissionOperation::Propose,
                entitybase::RolePermissionOperation::Vote
            ]),
            state,
            Some(2)
        ));
        assert_eq!(
            pallet::InstitutionGovernanceThresholds::<Test>::get(&cid),
            Some(2)
        );
        assert_noop!(
            register(
                entitybase::InstitutionRoleRegistration {
                    role_mutations: vec![],
                    assignment_changes: vec![]
                },
                PrivateEntity::registration_state_hash(&cid).unwrap(),
                Some(2)
            ),
            Error::<Test>::AlreadyInitialized
        );
    });
}

#[test]
fn policy_rejects_oversized_changes_and_permission_sets_without_writes() {
    new_test_ext().execute_with(|| {
        let (cid, role) = policy_fixture("policy-capacity");
        let state = PrivateEntity::registration_state_hash(&cid).unwrap();
        let mut action = policy_action(&role);
        action.permission_changes = vec![action.permission_changes[0].clone(); 65];
        assert_noop!(
            PrivateEntity::propose_institution_policy(
                RuntimeOrigin::signed(admin(1)),
                cid.clone(),
                action,
                policy_proposer(),
                state
            ),
            Error::<Test>::TooManyGovernanceChanges
        );
        let mut action = policy_action(&role);
        action.permission_changes[0].permissions = (0..257)
            .map(|action_code| entitybase::RolePermissionSpec {
                business_action_id: entitybase::BusinessActionId {
                    module_tag: b"x".to_vec(),
                    action_code,
                },
                operation: entitybase::RolePermissionOperation::Propose,
            })
            .collect();
        assert_noop!(
            PrivateEntity::propose_institution_policy(
                RuntimeOrigin::signed(admin(1)),
                cid.clone(),
                action,
                policy_proposer(),
                state
            ),
            Error::<Test>::TooManyRolePermissions
        );
        assert_eq!(PrivateEntity::registration_state_hash(&cid).unwrap(), state);
    });
}

//! 验证公权机构治理、固定岗位和管理员登记的完整链状态。

use super::*;
use frame_support::traits::GetCallName;
use frame_support::{assert_noop, assert_ok};

use crate::{AccountKind, Error, RESERVED_NAME_FEE, RESERVED_NAME_MAIN};

const ACCOUNT_AMOUNT: Balance = 1_000;
const REGISTRY_FUNDING_BALANCE: Balance = 100_000;
const CUSTOM_ACCOUNT_NAME: &[u8] = "专项账户".as_bytes();

#[test]
fn direct_institution_creation_call_is_permanently_absent() {
    let calls = <pallet::Call<Test> as GetCallName>::get_call_names();
    assert!(!calls.contains(&"propose_create_public_institution"));
}

fn governance_assignment(
    account_id: AccountId32,
    term_start: u32,
    term_end: u32,
) -> entitybase::InstitutionAssignmentTarget {
    entitybase::InstitutionAssignmentTarget {
        person_cid: fixture_person(&account_id, "CTZN").to_vec(),
        term_start,
        term_end,
        assignment_source: entitybase::InstitutionAssignmentSource::InstitutionGovernance,
        assignment_source_ref: b"proposal-result".to_vec(),
        assignment_status: entitybase::InstitutionAssignmentStatus::Active,
    }
}

fn governance_permission(
    operation: entitybase::RolePermissionOperation,
) -> entitybase::RolePermissionSpec {
    entitybase::RolePermissionSpec {
        business_action_id: entitybase::BusinessActionId {
            module_tag: b"pub-mgmt".to_vec(),
            action_code: 3,
        },
        operation,
    }
}

fn fund_registry_account() {
    assert_ok!(Balances::force_set_balance(
        RuntimeOrigin::root(),
        registry_funding_account(),
        REGISTRY_FUNDING_BALANCE,
    ));
}

fn create_cgov(tag: &str) -> pallet::CidNumberOf<Test> {
    let cid = generated_cid(tag, "CGOV");
    let institution_code = code_bytes("CGOV");
    // 测试机构均非 UNIN，无父级可传。
    let protocol_accounts =
        crate::tests::accounts::build_required_protocol_accounts::<Test>(&cid, None)
            .expect("测试协议账户必须可构造");
    let (created_accounts, _, _, _) =
        crate::tests::accounts::validate_initial_accounts::<Test>(&cid, &protocol_accounts, None)
            .expect("测试协议账户必须合法");
    pallet::Institutions::<Test>::insert(
        &cid,
        crate::InstitutionInfo {
            cid_full_name: cid_full_name("测试公权机构".as_bytes()),
            cid_short_name: cid_short_name("测试机构".as_bytes()),
            town_code: empty_town_code(),
            legal_representative: None,
            institution_code,
            created_at: System::block_number(),
        },
    );
    // 固定岗位权限由机构 CID + 岗位码推导，因此先落机构身份，再创建默认 LR 岗位。
    assert_ok!(PublicEntity::store_default_legal_representative_role(&cid));
    for account_id in created_accounts {
        pallet::InstitutionAccounts::<Test>::insert(
            &cid,
            &account_id.account_name,
            crate::InstitutionAccountInfo {
                account_id: account_id.account_id.clone(),
                initial_balance: account_id.amount,
                created_at: System::block_number(),
            },
        );
        pallet::AccountRegisteredCid::<Test>::insert(
            &account_id.account_id,
            crate::RegisteredInstitution {
                cid_number: cid.clone(),
                account_name: account_id.account_name,
            },
        );
    }
    grant_governance_role(&cid);
    pallet::InstitutionGovernanceThresholds::<Test>::insert(&cid, 2);
    cid
}

fn create_cgov_with_custom(tag: &str) -> pallet::CidNumberOf<Test> {
    fund_registry_account();
    let cid = create_cgov(tag);
    grant_close_role(&cid);
    // 新增账户已改为机构自身提案+投票流程;关闭账户测试的 setup 直接落库一个自定义账户,
    // 不再依赖新增账户投票路径(新增流程本身由本文件的 add_account_* 用例覆盖)。
    insert_custom_account(&cid, CUSTOM_ACCOUNT_NAME);
    assert_ok!(Balances::force_set_balance(
        RuntimeOrigin::root(),
        account_of(&cid, CUSTOM_ACCOUNT_NAME),
        ACCOUNT_AMOUNT,
    ));
    assert_ok!(Balances::force_set_balance(
        RuntimeOrigin::root(),
        account_of(&cid, RESERVED_NAME_MAIN),
        ACCOUNT_AMOUNT,
    ));
    assert_ok!(Balances::force_set_balance(
        RuntimeOrigin::root(),
        account_of(&cid, RESERVED_NAME_FEE),
        ACCOUNT_AMOUNT,
    ));
    cid
}

#[test]
fn dynamic_role_lifecycle_persists_permissions_and_never_reuses_code() {
    new_test_ext().execute_with(|| {
        use entitybase::{
            InstitutionRoleAuthorizationQuery, InstitutionRoleMutation, InstitutionRoleQuery,
            RolePermissionOperation, RoleSubject,
        };

        let cid = create_cgov("dynamic-role-lifecycle");
        let initial_nonce = pallet::InstitutionRoleNonce::<Test>::get(&cid);
        let first_code = entitybase::generate_dynamic_role_code(
            crate::MODULE_TAG,
            cid.as_slice(),
            initial_nonce,
            42,
        );
        let action = entitybase::BusinessActionId {
            module_tag: b"pub-mgmt".to_vec(),
            action_code: 3,
        };
        assert_ok!(PublicEntity::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: code_bytes("CGOV"),
                cid_number: cid.to_vec(),
                proposal_id: 42,
                role_mutations: vec![InstitutionRoleMutation::Create {
                    role_name: "财务负责人".as_bytes().to_vec(),
                    term_required: false, seats: 3,
                    permissions: vec![
                        governance_permission(RolePermissionOperation::Propose),
                        governance_permission(RolePermissionOperation::Vote),
                    ],
                    assignments: vec![governance_assignment(admin(0), 0, 0)],
                }],
                assignment_changes: vec![],
                legal_representative_change: None,
                result_source_ref: b"proposal-42".to_vec(),
            }
        ));

        let bounded_code: crate::institution::role::RoleCodeOf =
            first_code.clone().try_into().expect("code fits");
        assert_eq!(
            pallet::InstitutionRoleNonce::<Test>::get(&cid),
            initial_nonce + 1
        );
        assert!(pallet::UsedRoleCodes::<Test>::get(&cid, &bounded_code));
        assert!(pallet::InstitutionRoles::<Test>::contains_key(
            &cid,
            &bounded_code
        ));
        assert_eq!(
            pallet::InstitutionRolePermissions::<Test>::get(&cid, &bounded_code).len(),
            2
        );
        let subject = RoleSubject {
            cid_number: cid.to_vec(),
            role_code: first_code.clone(),
        };
        assert!(<PublicEntity as InstitutionRoleAuthorizationQuery<
            AccountId32,
        >>::is_authorized(
            &admin(0),
            &subject,
            &action,
            RolePermissionOperation::Propose,
        ));
        assert!(!<PublicEntity as InstitutionRoleAuthorizationQuery<
            AccountId32,
        >>::is_authorized(
            &admin(9),
            &subject,
            &action,
            RolePermissionOperation::Propose,
        ));

        assert_ok!(PublicEntity::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: code_bytes("CGOV"),
                cid_number: cid.to_vec(),
                proposal_id: 43,
                role_mutations: vec![InstitutionRoleMutation::Rename {
                    role_code: first_code.clone(),
                    role_name: "资金负责人".as_bytes().to_vec(),
                }],
                assignment_changes: vec![],
                legal_representative_change: None,
                result_source_ref: b"proposal-43".to_vec(),
            }
        ));
        assert_eq!(
            pallet::InstitutionRoles::<Test>::get(&cid, &bounded_code)
                .expect("role exists")
                .role_name
                .as_slice(),
            "资金负责人".as_bytes()
        );

        assert_ok!(PublicEntity::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: code_bytes("CGOV"),
                cid_number: cid.to_vec(),
                proposal_id: 44,
                role_mutations: vec![InstitutionRoleMutation::Delete {
                    role_code: first_code.clone(),
                }],
                assignment_changes: vec![],
                legal_representative_change: None,
                result_source_ref: b"proposal-44".to_vec(),
            }
        ));
        assert!(!pallet::InstitutionRoles::<Test>::contains_key(
            &cid,
            &bounded_code
        ));
        assert!(pallet::InstitutionRolePermissions::<Test>::get(&cid, &bounded_code).is_empty());
        assert!(pallet::InstitutionRoleAssignments::<Test>::get(&cid, &bounded_code).is_empty());
        assert!(pallet::UsedRoleCodes::<Test>::get(&cid, &bounded_code));

        let second_nonce = pallet::InstitutionRoleNonce::<Test>::get(&cid);
        assert_ok!(PublicEntity::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: code_bytes("CGOV"),
                cid_number: cid.to_vec(),
                proposal_id: 42,
                role_mutations: vec![InstitutionRoleMutation::Create {
                    role_name: "新岗位".as_bytes().to_vec(),
                    term_required: false, seats: 3,
                    permissions: vec![governance_permission(RolePermissionOperation::Propose)],
                    assignments: vec![],
                }],
                assignment_changes: vec![],
                legal_representative_change: None,
                result_source_ref: b"proposal-42-second".to_vec(),
            }
        ));
        let second_code = entitybase::generate_dynamic_role_code(
            crate::MODULE_TAG,
            cid.as_slice(),
            second_nonce,
            42,
        );
        assert_ne!(first_code, second_code);
        assert!(pallet::UsedRoleCodes::<Test>::get(
            &cid,
            crate::institution::role::RoleCodeOf::try_from(second_code).expect("code fits")
        ));
        assert!(
            !<PublicEntity as InstitutionRoleQuery<AccountId32>>::is_active_assignment(
                cid.as_slice(),
                &admin(0),
                first_code.as_slice(),
            )
        );
    });
}


#[test]
fn dynamic_role_name_cannot_duplicate_fixed_legal_representative_role() {
    new_test_ext().execute_with(|| {
        use entitybase::{InstitutionRoleMutation, RolePermissionOperation};

        let cid = create_cgov("duplicate-lr-name");
        assert_noop!(
            PublicEntity::apply_institution_governance_result(
                entitybase::InstitutionGovernanceResult {
                    institution_code: code_bytes("CGOV"),
                    cid_number: cid.to_vec(),
                    proposal_id: 78,
                    role_mutations: vec![InstitutionRoleMutation::Create {
                        role_name:
                            primitives::institution_constraints::ROLE_NAME_LEGAL_REPRESENTATIVE
                                .to_vec(),
                        term_required: false, seats: 3,
                        permissions: vec![governance_permission(RolePermissionOperation::Propose,)],
                        assignments: vec![],
                    }],
                    assignment_changes: vec![],
                    legal_representative_change: None,
                    result_source_ref: b"proposal-78".to_vec(),
                }
            ),
            Error::<Test>::DuplicateRoleName
        );
    });
}

#[test]
fn assignment_authorization_respects_inclusive_term_window() {
    new_test_ext().execute_with(|| {
        use entitybase::{InstitutionRoleMutation, InstitutionRoleQuery, RolePermissionOperation};

        let cid = create_cgov("dynamic-role-term");
        let role_code = entitybase::generate_dynamic_role_code(
            crate::MODULE_TAG,
            cid.as_slice(),
            pallet::InstitutionRoleNonce::<Test>::get(&cid),
            77,
        );
        assert_ok!(PublicEntity::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: code_bytes("CGOV"),
                cid_number: cid.to_vec(),
                proposal_id: 77,
                role_mutations: vec![InstitutionRoleMutation::Create {
                    role_name: "任期岗位".as_bytes().to_vec(),
                    term_required: true, seats: 3,
                    permissions: vec![governance_permission(RolePermissionOperation::Vote)],
                    assignments: vec![
                        governance_assignment(admin(0), 20_635, 20_635),
                        governance_assignment(admin(1), 20_600, 20_634),
                    ],
                }],
                assignment_changes: vec![],
                legal_representative_change: None,
                result_source_ref: b"proposal-77".to_vec(),
            }
        ));

        assert!(
            <PublicEntity as InstitutionRoleQuery<AccountId32>>::is_active_assignment(
                cid.as_slice(),
                &admin(0),
                role_code.as_slice(),
            )
        );
        assert!(
            !<PublicEntity as InstitutionRoleQuery<AccountId32>>::is_active_assignment(
                cid.as_slice(),
                &admin(1),
                role_code.as_slice(),
            )
        );
    });
}

fn account_of(cid: &pallet::CidNumberOf<Test>, name: &[u8]) -> AccountId32 {
    pallet::InstitutionAccounts::<Test>::get(cid, account_name(name))
        .expect("institution account_id must exist")
        .account_id
}

#[test]
fn update_info_and_add_account_keep_cid_as_only_entity_key() {
    new_test_ext().execute_with(|| {
        let cid = create_cgov_with_custom("maintain-cid");
        // 改名仍由注册局直写。
        assert_ok!(PublicEntity::update_institution_info(
            RuntimeOrigin::signed(creator_account_id()),
            cid.clone(),
            cid_full_name("更新后的机构全称".as_bytes()),
            cid_short_name("更新简称".as_bytes()),
            b"REGISTRY-CID".to_vec(),
            b"REGISTRY-ROLE".to_vec(),
        ));
        let updated = pallet::Institutions::<Test>::get(&cid).expect("institution remains");
        assert_eq!(
            updated.cid_full_name.as_slice(),
            "更新后的机构全称".as_bytes()
        );

        // 新增账户改为本机构提案 → 内部投票通过 → finalizer 落库。
        let added_name = "新增账户".as_bytes();
        assert_ok!(propose_add_custom_account(
            RuntimeOrigin::signed(admin(0)),
            cid.clone(),
            &[added_name],
        ));
        let proposal_id = last_proposal_id();
        assert_ok!(cast_yes_votes(&[admin(1), admin(2)], 2, proposal_id));
        let added = account_of(&cid, added_name);
        assert_eq!(
            pallet::AccountRegisteredCid::<Test>::get(&added)
                .expect("new reverse index")
                .cid_number,
            cid
        );
        assert_eq!(Balances::free_balance(added), 0);
    });
}

#[test]
fn add_account_rejects_protocol_names_and_duplicate_custom_names() {
    new_test_ext().execute_with(|| {
        // 保留名/重复名在发起阶段即被派生校验链拒绝,不写 Pending、不建提案。
        let cid = create_cgov_with_custom("add-invalid");
        assert_noop!(
            propose_add_custom_account(
                RuntimeOrigin::signed(admin(0)),
                cid.clone(),
                &[RESERVED_NAME_MAIN],
            ),
            Error::<Test>::ReservedAccountName
        );
        assert_noop!(
            propose_add_custom_account(
                RuntimeOrigin::signed(admin(0)),
                cid,
                &["重复账户".as_bytes(), "重复账户".as_bytes()],
            ),
            Error::<Test>::DuplicateAccountName
        );
    });
}

#[test]
fn add_account_proposal_then_vote_inserts_account() {
    new_test_ext().execute_with(|| {
        let cid = create_cgov("add-vote");
        grant_close_role(&cid);
        let new_name = "投票新增账户".as_bytes();
        assert!(!pallet::InstitutionAccounts::<Test>::contains_key(
            &cid,
            account_name(new_name),
        ));

        assert_ok!(propose_add_custom_account(
            RuntimeOrigin::signed(admin(0)),
            cid.clone(),
            &[new_name],
        ));
        let proposal_id = last_proposal_id();
        // 发起后 Pending 命中,尚未落库。
        assert_eq!(
            pallet::InstitutionPendingAdd::<Test>::get(&cid),
            Some(proposal_id)
        );
        assert!(!pallet::InstitutionAccounts::<Test>::contains_key(
            &cid,
            account_name(new_name),
        ));

        assert_ok!(cast_yes_votes(&[admin(1), admin(2)], 2, proposal_id));

        // 通过后账户落库、反向索引写入、Pending 清除。
        let added = account_of(&cid, new_name);
        assert!(
            sp_io::storage::exists(&codec::Encode::encode(&(
                b"evm-receive-registration",
                &added
            ))),
            "zero-balance business registration must invoke the receive-alias hook"
        );
        assert_eq!(
            pallet::AccountRegisteredCid::<Test>::get(&added)
                .expect("new reverse index")
                .cid_number,
            cid
        );
        assert_eq!(Balances::free_balance(&added), 0);
        assert!(!pallet::InstitutionPendingAdd::<Test>::contains_key(&cid));
    });
}

#[test]
fn add_account_requires_institution_admin_and_role() {
    new_test_ext().execute_with(|| {
        let cid = create_cgov("add-auth");
        grant_close_role(&cid);
        // 非本机构管理员账户发起 → build_institution_vote_plan 授权失败。
        assert_noop!(
            propose_add_custom_account(
                RuntimeOrigin::signed(admin(9)),
                cid.clone(),
                &["越权账户".as_bytes()],
            ),
            Error::<Test>::PermissionDenied
        );
        // 不存在的机构 → InstitutionNotFound。
        let ghost = generated_cid("add-auth-ghost", "CGOV");
        assert_noop!(
            propose_add_custom_account(
                RuntimeOrigin::signed(admin(0)),
                ghost,
                &["幽灵账户".as_bytes()],
            ),
            Error::<Test>::InstitutionNotFound
        );
    });
}

#[test]
fn duplicate_add_proposal_is_rejected_while_pending() {
    new_test_ext().execute_with(|| {
        let cid = create_cgov("add-pending");
        grant_close_role(&cid);
        // 首次发起成功后 InstitutionPendingAdd 命中,同机构重复发起新增必须被拒。
        assert_ok!(propose_add_custom_account(
            RuntimeOrigin::signed(admin(0)),
            cid.clone(),
            &["账户甲".as_bytes()],
        ));
        assert_noop!(
            propose_add_custom_account(
                RuntimeOrigin::signed(admin(0)),
                cid,
                &["账户乙".as_bytes()],
            ),
            Error::<Test>::AddAlreadyPending
        );
    });
}

#[test]
fn rejected_add_is_cleaned_only_by_engingcore_callback() {
    new_test_ext().execute_with(|| {
        let cid = create_cgov("add-rejected");
        grant_close_role(&cid);
        let new_name = "被否新增账户".as_bytes();
        assert_ok!(propose_add_custom_account(
            RuntimeOrigin::signed(admin(0)),
            cid.clone(),
            &[new_name],
        ));
        let proposal_id = last_proposal_id();
        assert_eq!(
            pallet::InstitutionPendingAdd::<Test>::get(&cid),
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
        assert!(!pallet::InstitutionPendingAdd::<Test>::contains_key(&cid));
        assert!(!pallet::InstitutionAccounts::<Test>::contains_key(
            &cid,
            account_name(new_name),
        ));
    });
}

#[test]
fn derive_account_distinguishes_protocol_and_custom_account_kinds() {
    new_test_ext().execute_with(|| {
        let cid = generated_cid("derive-kinds", "CGOV");
        let (_, main_kind) =
            PublicEntity::derive_institution_account(cid.as_slice(), RESERVED_NAME_MAIN).unwrap();
        let (_, fee_kind) =
            PublicEntity::derive_institution_account(cid.as_slice(), RESERVED_NAME_FEE).unwrap();
        let (_, custom_kind) =
            PublicEntity::derive_institution_account(cid.as_slice(), CUSTOM_ACCOUNT_NAME).unwrap();
        assert!(matches!(main_kind, AccountKind::InstitutionMain { .. }));
        assert!(matches!(fee_kind, AccountKind::InstitutionFee { .. }));
        assert!(matches!(custom_kind, AccountKind::InstitutionNamed { .. }));
    });
}

#[test]
fn close_requires_matching_actor_cid_and_an_institution_admin() {
    new_test_ext().execute_with(|| {
        let cid = create_cgov_with_custom("close-auth");
        let custom = account_of(&cid, CUSTOM_ACCOUNT_NAME);
        let other_cid = generated_cid("close-auth-other", "CGOV");
        assert_noop!(
            propose_named_account_close(
                RuntimeOrigin::signed(admin(0)),
                other_cid,
                custom.clone(),
                beneficiary_account_id(),
            ),
            Error::<Test>::NotInstitutionAccount
        );
        assert_noop!(
            propose_named_account_close(
                RuntimeOrigin::signed(admin(9)),
                cid,
                custom,
                beneficiary_account_id(),
            ),
            Error::<Test>::PermissionDenied
        );
    });
}

#[test]
fn protocol_accounts_cannot_be_closed() {
    new_test_ext().execute_with(|| {
        let cid = create_cgov_with_custom("close-protocol");
        for account_id in [
            account_of(&cid, RESERVED_NAME_MAIN),
            account_of(&cid, RESERVED_NAME_FEE),
        ] {
            assert_noop!(
                propose_named_account_close(
                    RuntimeOrigin::signed(admin(0)),
                    cid.clone(),
                    account_id,
                    beneficiary_account_id(),
                ),
                Error::<Test>::CannotCloseProtectedInstitution
            );
        }
    });
}

#[test]
fn approved_close_removes_only_custom_account() {
    new_test_ext().execute_with(|| {
        let cid = create_cgov_with_custom("close-custom");
        let main = account_of(&cid, RESERVED_NAME_MAIN);
        let fee = account_of(&cid, RESERVED_NAME_FEE);
        let custom = account_of(&cid, CUSTOM_ACCOUNT_NAME);
        let beneficiary_account_id = beneficiary_account_id();
        let admin_balance_before = Balances::free_balance(admin(0));

        assert_ok!(propose_named_account_close(
            RuntimeOrigin::signed(admin(0)),
            cid.clone(),
            custom.clone(),
            beneficiary_account_id.clone(),
        ));
        let proposal_id = last_proposal_id();
        assert_ok!(cast_yes_votes(&[admin(1), admin(2)], 2, proposal_id));

        assert!(!pallet::AccountRegisteredCid::<Test>::contains_key(&custom));
        assert!(!pallet::InstitutionAccounts::<Test>::contains_key(
            &cid,
            account_name(CUSTOM_ACCOUNT_NAME),
        ));
        assert!(pallet::AccountRegisteredCid::<Test>::contains_key(&main));
        assert!(pallet::AccountRegisteredCid::<Test>::contains_key(&fee));
        assert!(pallet::Institutions::<Test>::contains_key(&cid));

        assert_eq!(Balances::free_balance(&fee), 990);
        assert_eq!(
            Balances::free_balance(beneficiary_account_id),
            ACCOUNT_AMOUNT
        );
        assert_eq!(Balances::free_balance(admin(0)), admin_balance_before);
    });
}

#[test]
fn rejected_close_is_cleaned_only_by_engingcore_callback() {
    new_test_ext().execute_with(|| {
        let cid = create_cgov_with_custom("close-rejected");
        let custom = account_of(&cid, CUSTOM_ACCOUNT_NAME);

        assert_ok!(propose_named_account_close(
            RuntimeOrigin::signed(admin(0)),
            cid.clone(),
            custom.clone(),
            beneficiary_account_id(),
        ));
        let proposal_id = last_proposal_id();
        assert_eq!(
            pallet::InstitutionPendingClose::<Test>::get(&custom),
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
            &custom
        ));
        assert!(pallet::InstitutionAccounts::<Test>::contains_key(
            &cid,
            account_name(CUSTOM_ACCOUNT_NAME),
        ));
    });
}

#[test]
fn duplicate_close_proposal_is_rejected_while_pending() {
    new_test_ext().execute_with(|| {
        let cid = create_cgov_with_custom("close-pending");
        let custom = account_of(&cid, CUSTOM_ACCOUNT_NAME);
        // 首次发起成功后 InstitutionPendingClose 命中,重复发起同账户关闭必须被拒。
        assert_ok!(propose_named_account_close(
            RuntimeOrigin::signed(admin(0)),
            cid.clone(),
            custom.clone(),
            beneficiary_account_id(),
        ));
        assert_noop!(
            propose_named_account_close(
                RuntimeOrigin::signed(admin(0)),
                cid,
                custom,
                beneficiary_account_id(),
            ),
            Error::<Test>::CloseAlreadyPending
        );
    });
}

#[test]
fn public_role_rejects_resident_and_follows_citizen_wallet_rebinding() {
    use entitybase::{InstitutionRoleAssignmentChange, InstitutionRoleQuery};
    new_test_ext().execute_with(|| {
        let cid = create_cgov("public-person-binding");
        let role = grant_close_role(&cid);
        let current = admin(0);
        let next = admin(9);
        let person_cid = fixture_person(&current, "CTZN");
        assert!(<PublicEntity as InstitutionRoleQuery<AccountId32>>::is_active_assignment(&cid, &current, &role));
        bind_test_wallet(&person_cid, next.clone());
        assert!(!<PublicEntity as InstitutionRoleQuery<AccountId32>>::is_active_assignment(&cid, &current, &role));
        assert!(<PublicEntity as InstitutionRoleQuery<AccountId32>>::is_active_assignment(&cid, &next, &role));
        let resident = fixture_person(&admin(10), "NATP");
        let mut assignment = governance_assignment(next, 0, 0);
        assignment.person_cid = resident.to_vec();
        assert_noop!(
            PublicEntity::apply_institution_governance_result(
                entitybase::InstitutionGovernanceResult {
                    institution_code: *b"CGOV",
                    cid_number: cid.to_vec(),
                    proposal_id: 90,
                    role_mutations: vec![],
                    assignment_changes: vec![InstitutionRoleAssignmentChange {
                        role_code: role.to_vec(),
                        assignments: vec![assignment],
                    }],
                    legal_representative_change: None,
                    result_source_ref: b"fixture".to_vec(),
                }
            ),
            Error::<Test>::InvalidAssignmentPerson
        );
    });
}

// 原生政策回归：真实建案、原阈值投票、回调执行和失败终态。
fn policy_fixture(tag: &str) -> (pallet::CidNumberOf<Test>, crate::RoleCodeOf) {
    let cid = create_cgov(tag);
    let nonce = pallet::InstitutionRoleNonce::<Test>::get(&cid);
    let raw = entitybase::generate_dynamic_role_code(crate::MODULE_TAG, &cid, nonce, 900);
    let role: crate::RoleCodeOf = raw.clone().try_into().unwrap();
    assert_ok!(PublicEntity::apply_institution_governance_result(
        entitybase::InstitutionGovernanceResult {
            institution_code: code_bytes("CGOV"),
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
    b"TEST_GOVERNANCE_ROLE".to_vec().try_into().unwrap()
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
        let state = PublicEntity::registration_state_hash(&cid).unwrap();
        assert_noop!(
            PublicEntity::propose_institution_policy(
                RuntimeOrigin::signed(admin(9)),
                cid.clone(),
                policy_action(&role),
                policy_proposer(),
                state
            ),
            Error::<Test>::PermissionDenied
        );
        assert_noop!(
            PublicEntity::propose_institution_policy(
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
            PublicEntity::propose_institution_policy(
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
            PublicEntity::propose_institution_policy(
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
                PublicEntity::propose_institution_policy(
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
            PublicEntity::propose_institution_policy(
                RuntimeOrigin::signed(admin(1)),
                cid.clone(),
                fixed,
                policy_proposer(),
                state
            ),
            Error::<Test>::InstitutionPolicyDenied
        );
        assert_eq!(PublicEntity::registration_state_hash(&cid).unwrap(), state);
        assert_eq!(VotingEngine::next_proposal_id(), 0);
    });
}

#[test]
fn policy_uses_old_people_and_threshold_then_executes_after_passed() {
    new_test_ext().execute_with(|| {
        let (cid, role) = policy_fixture("policy-execute");
        let state = PublicEntity::registration_state_hash(&cid).unwrap();
        assert_ok!(PublicEntity::propose_institution_policy(
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
        let old = admin(0);
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
        let state = PublicEntity::registration_state_hash(&cid).unwrap();
        assert_ok!(PublicEntity::propose_institution_policy(
            RuntimeOrigin::signed(admin(1)),
            cid.clone(),
            policy_action(&role),
            policy_proposer(),
            state
        ));
        let id = VotingEngine::next_proposal_id() - 1;
        assert_ok!(PublicEntity::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: code_bytes("CGOV"),
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
        assert_ok!(policy_cast(admin(0), id));
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
                institution_code: code_bytes("CGOV"),
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
        let state = PublicEntity::registration_state_hash(&cid).unwrap();
        assert_noop!(
            PublicEntity::apply_institution_governance_result(result(
                2,
                false,
                entitybase::InstitutionRoleStatus::Active,
                vec![]
            )),
            Error::<Test>::FixedRoleSeatsMismatch
        );
        assert_noop!(
            PublicEntity::apply_institution_governance_result(result(
                5,
                true,
                entitybase::InstitutionRoleStatus::Active,
                vec![]
            )),
            Error::<Test>::InvalidAssignmentTerm
        );
        assert_eq!(PublicEntity::registration_state_hash(&cid).unwrap(), state);
        let assignments = (0..2).map(|n| policy_assignment(admin(n))).collect();
        assert_ok!(PublicEntity::apply_institution_governance_result(result(
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
        assert_ok!(PublicEntity::apply_institution_governance_result(result(
            2,
            true,
            entitybase::InstitutionRoleStatus::Active,
            vec![entitybase::InstitutionRoleAssignmentChange {
                role_code: role.to_vec(),
                assignments
            }]
        )));
        assert_eq!(PublicEntity::snapshot_role_persons(&cid, &role).len(), 2);
        let state = PublicEntity::registration_state_hash(&cid).unwrap();
        let assignments = (0..2)
            .map(|n| {
                let mut assignment = policy_assignment(admin(n));
                assignment.term_start = today + 1;
                assignment.term_end = today + 1;
                assignment
            })
            .collect();
        assert_noop!(
            PublicEntity::apply_institution_governance_result(result(
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
        assert_eq!(PublicEntity::registration_state_hash(&cid).unwrap(), state);
        assert_ok!(PublicEntity::apply_institution_governance_result(result(
            2,
            false,
            entitybase::InstitutionRoleStatus::Inactive,
            vec![entitybase::InstitutionRoleAssignmentChange {
                role_code: role.to_vec(),
                assignments: vec![]
            }]
        )));
        assert!(PublicEntity::snapshot_role_persons(&cid, &role).is_empty());
        assert_ok!(PublicEntity::propose_institution_policy(
            RuntimeOrigin::signed(admin(1)),
            cid.clone(),
            entitybase::InstitutionPolicyAction {
                permission_changes: vec![],
                threshold: Some(2)
            },
            policy_proposer(),
            PublicEntity::registration_state_hash(&cid).unwrap()
        ));
        assert_ok!(PublicEntity::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: code_bytes("CGOV"),
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
        let state = PublicEntity::registration_state_hash(&cid).unwrap();
        assert_noop!(
            PublicEntity::propose_institution_policy(
                RuntimeOrigin::signed(admin(1)),
                cid.clone(),
                policy_action(&role),
                policy_proposer(),
                state
            ),
            Error::<Test>::InvalidAssignmentPerson
        );
        assert_eq!(PublicEntity::registration_state_hash(&cid).unwrap(), state);
    });
}

#[test]
fn common_threshold_rejects_disjoint_electorates_without_partial_roles() {
    new_test_ext().execute_with(|| {
        let (cid, role) = policy_fixture("policy-threshold");
        let state = PublicEntity::registration_state_hash(&cid).unwrap();
        let assignments = (0..5).map(|n| policy_assignment(admin(n))).collect();
        assert_noop!(
            PublicEntity::apply_institution_governance_result(
                entitybase::InstitutionGovernanceResult {
                    institution_code: code_bytes("CGOV"),
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
        assert_eq!(PublicEntity::registration_state_hash(&cid).unwrap(), state);
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
        let state = PublicEntity::registration_state_hash(&cid).unwrap();
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
                    assignments: [admin(1), admin(0)]
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
            PublicEntity::register_institution_roles(
                RuntimeOrigin::signed(creator_account_id()),
                cid.clone(),
                action,
                b"REGISTRY-CID".to_vec(),
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
        assert_eq!(PublicEntity::registration_state_hash(&cid).unwrap(), state);
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
                PublicEntity::registration_state_hash(&cid).unwrap(),
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
        let state = PublicEntity::registration_state_hash(&cid).unwrap();
        let mut action = policy_action(&role);
        action.permission_changes = vec![action.permission_changes[0].clone(); 65];
        assert_noop!(
            PublicEntity::propose_institution_policy(
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
            PublicEntity::propose_institution_policy(
                RuntimeOrigin::signed(admin(1)),
                cid.clone(),
                action,
                policy_proposer(),
                state
            ),
            Error::<Test>::TooManyRolePermissions
        );
        assert_eq!(PublicEntity::registration_state_hash(&cid).unwrap(), state);
    });
}

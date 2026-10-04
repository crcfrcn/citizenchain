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
) -> entity_primitives::InstitutionAssignmentTarget<AccountId32> {
    entity_primitives::InstitutionAssignmentTarget {
        account_id,
        term_start,
        term_end,
        assignment_source: entity_primitives::InstitutionAssignmentSource::InstitutionGovernance,
        assignment_source_ref: b"proposal-result".to_vec(),
        assignment_status: entity_primitives::InstitutionAssignmentStatus::Active,
    }
}

fn governance_permission(
    operation: entity_primitives::RolePermissionOperation,
) -> entity_primitives::RolePermissionSpec {
    entity_primitives::RolePermissionSpec {
        business_action_id: entity_primitives::BusinessActionId {
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
        crate::institution::accounts::build_required_protocol_accounts::<Test>(&cid, None)
            .expect("测试协议账户必须可构造");
    let (created_accounts, _, _, _) =
        crate::institution::accounts::validate_initial_accounts::<Test>(
            &cid,
            &protocol_accounts,
            None,
        )
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
    assert_ok!(PublicManage::store_default_legal_representative_role(&cid));
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
    let admins = institution_admins(3);
    assert_ok!(PublicManage::set_institution_admins(
        &cid,
        institution_code,
        &admins,
    ));
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
        use entity_primitives::{
            InstitutionRoleAuthorizationQuery, InstitutionRoleMutation, InstitutionRoleQuery,
            RolePermissionOperation, RoleSubject,
        };

        let cid = create_cgov("dynamic-role-lifecycle");
        let first_code =
            entity_primitives::generate_dynamic_role_code(crate::MODULE_TAG, cid.as_slice(), 0, 42);
        let action = entity_primitives::BusinessActionId {
            module_tag: b"pub-mgmt".to_vec(),
            action_code: 3,
        };
        assert_ok!(PublicManage::apply_institution_governance_result(
            entity_primitives::InstitutionGovernanceResult {
                institution_code: code_bytes("CGOV"),
                cid_number: cid.to_vec(),
                proposal_id: 42,
                role_mutations: vec![InstitutionRoleMutation::Create {
                    role_name: "财务负责人".as_bytes().to_vec(),
                    term_required: false,
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
        assert_eq!(pallet::InstitutionRoleNonce::<Test>::get(&cid), 1);
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
        assert!(<PublicManage as InstitutionRoleAuthorizationQuery<
            AccountId32,
        >>::is_authorized(
            &admin(0),
            &subject,
            &action,
            RolePermissionOperation::Propose,
        ));
        assert!(!<PublicManage as InstitutionRoleAuthorizationQuery<
            AccountId32,
        >>::is_authorized(
            &admin(9),
            &subject,
            &action,
            RolePermissionOperation::Propose,
        ));

        assert_ok!(PublicManage::apply_institution_governance_result(
            entity_primitives::InstitutionGovernanceResult {
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

        assert_ok!(PublicManage::apply_institution_governance_result(
            entity_primitives::InstitutionGovernanceResult {
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

        assert_ok!(PublicManage::apply_institution_governance_result(
            entity_primitives::InstitutionGovernanceResult {
                institution_code: code_bytes("CGOV"),
                cid_number: cid.to_vec(),
                proposal_id: 42,
                role_mutations: vec![InstitutionRoleMutation::Create {
                    role_name: "新岗位".as_bytes().to_vec(),
                    term_required: false,
                    permissions: vec![governance_permission(RolePermissionOperation::Propose)],
                    assignments: vec![],
                }],
                assignment_changes: vec![],
                legal_representative_change: None,
                result_source_ref: b"proposal-42-second".to_vec(),
            }
        ));
        let second_code =
            entity_primitives::generate_dynamic_role_code(crate::MODULE_TAG, cid.as_slice(), 1, 42);
        assert_ne!(first_code, second_code);
        assert!(pallet::UsedRoleCodes::<Test>::get(
            &cid,
            crate::institution::role::RoleCodeOf::try_from(second_code).expect("code fits")
        ));
        assert!(
            !<PublicManage as InstitutionRoleQuery<AccountId32>>::is_active_assignment(
                cid.as_slice(),
                &admin(0),
                first_code.as_slice(),
            )
        );
    });
}

#[test]
fn operation_phase_authorizes_by_cid_and_survives_wallet_rebind() {
    new_test_ext().execute_with(|| {
        use entity_primitives::{
            InstitutionRoleAuthorizationQuery, InstitutionRoleMutation, RolePermissionOperation,
            RoleSubject,
        };

        let cid = create_cgov("cid-rebind");
        let institution_code = code_bytes("CGOV");
        let action = entity_primitives::BusinessActionId {
            module_tag: b"pub-mgmt".to_vec(),
            action_code: 3,
        };

        // 用带合法 CTZN CID 的名册覆盖：admin(0) 携 CID（先绑定 OLD 让 Phase 2 seed 校验通过），
        // admin(1)/admin(2) 无 CID（对照无 CID 语义）。
        let citizen_cid = b"CN220-CTZN2-198805200-2026".to_vec();
        bind_cid(&citizen_cid, admin(0));
        let admins: crate::InstitutionAdminsInputOf<Test> = vec![
            admin_primitives::Admin {
                account_id: admin(0),
                cid_number: citizen_cid.clone().try_into().expect("cid fits"),
                family_name: "张".as_bytes().to_vec().try_into().expect("family fits"),
                given_name: "三".as_bytes().to_vec().try_into().expect("given fits"),
            },
            admin_primitives::Admin {
                account_id: admin(1),
                cid_number: Default::default(),
                family_name: Default::default(),
                given_name: Default::default(),
            },
            admin_primitives::Admin {
                account_id: admin(2),
                cid_number: Default::default(),
                family_name: Default::default(),
                given_name: Default::default(),
            },
        ]
        .try_into()
        .expect("admins fit");
        assert_ok!(PublicManage::set_institution_admins(
            &cid,
            institution_code,
            &admins,
        ));

        // 建动态岗位，给 admin(0)（有 CID）与 admin(1)（无 CID）任职。
        let role_code =
            entity_primitives::generate_dynamic_role_code(crate::MODULE_TAG, cid.as_slice(), 0, 42);
        assert_ok!(PublicManage::apply_institution_governance_result(
            entity_primitives::InstitutionGovernanceResult {
                institution_code,
                cid_number: cid.to_vec(),
                proposal_id: 42,
                role_mutations: vec![InstitutionRoleMutation::Create {
                    role_name: "业务负责人".as_bytes().to_vec(),
                    term_required: false,
                    permissions: vec![governance_permission(RolePermissionOperation::Propose)],
                    assignments: vec![
                        governance_assignment(admin(0), 0, 0),
                        governance_assignment(admin(1), 0, 0),
                    ],
                }],
                assignment_changes: vec![],
                legal_representative_change: None,
                result_source_ref: b"proposal-rebind".to_vec(),
            }
        ));
        let subject = RoleSubject {
            cid_number: cid.to_vec(),
            role_code: role_code.clone(),
        };
        let authorized = |who: &AccountId32| {
            <PublicManage as InstitutionRoleAuthorizationQuery<AccountId32>>::is_authorized(
                who,
                &subject,
                &action,
                RolePermissionOperation::Propose,
            )
        };

        // 创世期：按 account_id 授权。
        assert!(authorized(&admin(0)));
        assert!(!authorized(&admin(5))); // 非名册钱包

        // 运行期：admin(0) 换绑到新钱包 admin(5)。
        set_operation_phase(true);
        bind_cid(&citizen_cid, admin(5));
        assert!(authorized(&admin(5))); // 换绑不掉权：新钱包解析到同一名册账户
        assert!(!authorized(&admin(0))); // 旧钱包掉权：CID 已不绑定它
        assert!(authorized(&admin(1))); // 无 CID 管理员运行期仍按 account_id
    });
}

#[test]
fn dynamic_role_name_cannot_duplicate_fixed_legal_representative_role() {
    new_test_ext().execute_with(|| {
        use entity_primitives::{InstitutionRoleMutation, RolePermissionOperation};

        let cid = create_cgov("duplicate-lr-name");
        assert_noop!(
            PublicManage::apply_institution_governance_result(
                entity_primitives::InstitutionGovernanceResult {
                    institution_code: code_bytes("CGOV"),
                    cid_number: cid.to_vec(),
                    proposal_id: 78,
                    role_mutations: vec![InstitutionRoleMutation::Create {
                        role_name:
                            primitives::institution_constraints::ROLE_NAME_LEGAL_REPRESENTATIVE
                                .to_vec(),
                        term_required: false,
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
        use entity_primitives::{
            InstitutionRoleMutation, InstitutionRoleQuery, RolePermissionOperation,
        };

        let cid = create_cgov("dynamic-role-term");
        let role_code =
            entity_primitives::generate_dynamic_role_code(crate::MODULE_TAG, cid.as_slice(), 0, 77);
        assert_ok!(PublicManage::apply_institution_governance_result(
            entity_primitives::InstitutionGovernanceResult {
                institution_code: code_bytes("CGOV"),
                cid_number: cid.to_vec(),
                proposal_id: 77,
                role_mutations: vec![InstitutionRoleMutation::Create {
                    role_name: "任期岗位".as_bytes().to_vec(),
                    term_required: true,
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
            <PublicManage as InstitutionRoleQuery<AccountId32>>::is_active_assignment(
                cid.as_slice(),
                &admin(0),
                role_code.as_slice(),
            )
        );
        assert!(
            !<PublicManage as InstitutionRoleQuery<AccountId32>>::is_active_assignment(
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
        assert_ok!(PublicManage::update_institution_info(
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
fn rejected_add_is_cleaned_only_by_votingengine_callback() {
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
            <crate::InternalVoteExecutor<Test> as votingengine::InternalVoteResultCallback>::on_internal_vote_finalized(
                proposal_id,
                false,
            ),
            Ok(votingengine::ProposalExecutionOutcome::Executed)
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
            PublicManage::derive_institution_account(cid.as_slice(), RESERVED_NAME_MAIN).unwrap();
        let (_, fee_kind) =
            PublicManage::derive_institution_account(cid.as_slice(), RESERVED_NAME_FEE).unwrap();
        let (_, custom_kind) =
            PublicManage::derive_institution_account(cid.as_slice(), CUSTOM_ACCOUNT_NAME).unwrap();
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
        assert!(public_admins::AdminAccounts::<Test>::contains_key(&cid));
        assert_eq!(Balances::free_balance(&fee), 990);
        assert_eq!(
            Balances::free_balance(beneficiary_account_id),
            ACCOUNT_AMOUNT
        );
        assert_eq!(Balances::free_balance(admin(0)), admin_balance_before);
    });
}

#[test]
fn rejected_close_is_cleaned_only_by_votingengine_callback() {
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
            <crate::InternalVoteExecutor<Test> as votingengine::InternalVoteResultCallback>::on_internal_vote_finalized(
                proposal_id,
                false,
            ),
            Ok(votingengine::ProposalExecutionOutcome::Executed)
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

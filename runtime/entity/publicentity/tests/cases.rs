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
) -> entitybase::InstitutionAssignmentTarget<AccountId32> {
    entitybase::InstitutionAssignmentTarget {
        account_id,
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
    assert_ok!(seed_institution_admins(&cid, institution_code, &admins,));
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
        assert_ok!(PublicManage::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
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

        assert_ok!(PublicManage::apply_institution_governance_result(
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
        assert_ok!(PublicManage::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
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
            !<PublicManage as InstitutionRoleQuery<AccountId32>>::is_active_assignment(
                cid.as_slice(),
                &admin(0),
                first_code.as_slice(),
            )
        );
    });
}

#[test]
fn initialized_institution_does_not_follow_personal_cid_wallet_rebind() {
    new_test_ext().execute_with(|| {
        use entitybase::{
            InstitutionRoleAuthorizationQuery, InstitutionRoleMutation, RolePermissionOperation,
            RoleSubject,
        };

        let cid = create_cgov("cid-rebind");
        let institution_code = code_bytes("CGOV");
        let action = entitybase::BusinessActionId {
            module_tag: b"pub-mgmt".to_vec(),
            action_code: 3,
        };

        // 用带合法 CTZN CID 的名册覆盖：admin(0) 携 CID（先绑定 OLD 让 Phase 2 seed 校验通过），
        // admin(1)/admin(2) 无 CID（对照无 CID 语义）。
        let citizen_cid = b"CN220-CTZN2-198805200-2026".to_vec();
        bind_cid(&citizen_cid, admin(0));
        let admins: crate::InstitutionAdminsInputOf<Test> = vec![
            adminbase::Admin {
                account_id: admin(0),
                cid_number: citizen_cid.clone().try_into().expect("cid fits"),
                family_name: "张".as_bytes().to_vec().try_into().expect("family fits"),
                given_name: "三".as_bytes().to_vec().try_into().expect("given fits"),
            },
            adminbase::Admin {
                account_id: admin(1),
                cid_number: Default::default(),
                family_name: Default::default(),
                given_name: Default::default(),
            },
            adminbase::Admin {
                account_id: admin(2),
                cid_number: Default::default(),
                family_name: Default::default(),
                given_name: Default::default(),
            },
        ]
        .try_into()
        .expect("admins fit");
        assert_ok!(seed_institution_admins(&cid, institution_code, &admins,));

        // 建动态岗位，给 admin(0)（有 CID）与 admin(1)（无 CID）任职。
        let role_code = entitybase::generate_dynamic_role_code(
            crate::MODULE_TAG,
            cid.as_slice(),
            pallet::InstitutionRoleNonce::<Test>::get(&cid),
            42,
        );
        assert_ok!(PublicManage::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
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
        assert!(!authorized(&admin(5))); // 个人换绑不会授予机构权限
        assert!(authorized(&admin(0))); // 机构钱包保持原授权
        assert!(authorized(&admin(1))); // 无 CID 管理员运行期仍按 account_id
    });
}

#[test]
fn dynamic_role_name_cannot_duplicate_fixed_legal_representative_role() {
    new_test_ext().execute_with(|| {
        use entitybase::{InstitutionRoleMutation, RolePermissionOperation};

        let cid = create_cgov("duplicate-lr-name");
        assert_noop!(
            PublicManage::apply_institution_governance_result(
                entitybase::InstitutionGovernanceResult {
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
        use entitybase::{InstitutionRoleMutation, InstitutionRoleQuery, RolePermissionOperation};

        let cid = create_cgov("dynamic-role-term");
        let role_code = entitybase::generate_dynamic_role_code(
            crate::MODULE_TAG,
            cid.as_slice(),
            pallet::InstitutionRoleNonce::<Test>::get(&cid),
            77,
        );
        assert_ok!(PublicManage::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
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
        assert!(publicadmin::AdminAccounts::<Test>::contains_key(&cid));
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

// 登记沿用统一机构模型。夹具在原测试文件内建立，不新增固定岗位/测试资产。
fn fresh_registration_target(tag: &str) -> pallet::CidNumberOf<Test> {
    let cid = create_cgov(tag);
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
    publicadmin::AdminAccounts::<Test>::remove(&cid);
    let _ = publicadmin::AdminWallets::<Test>::clear_prefix(&cid, u32::MAX, None);
    let _ = publicadmin::WalletRoleBindings::<Test>::clear_prefix(&cid, u32::MAX, None);
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
    PublicManage::register_institution_admins(
        RuntimeOrigin::signed(creator_account_id()),
        cid.clone(),
        action,
        b"REGISTRY-CID".to_vec(),
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
        let hash = PublicManage::registration_state_hash(&cid).unwrap();
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
            PublicManage::registration_state_hash(&cid).unwrap(),
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
            PublicManage::registration_state_hash(&cid).unwrap(),
            None
        ));
        assert_eq!(
            publicadmin::AdminAccounts::<Test>::get(&cid)
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
            PublicManage::registration_state_hash(&cid).unwrap(),
            None
        ));
        assert_eq!(
            publicadmin::AdminWallets::<Test>::get(&cid, admin(1))
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
            PublicManage::registration_state_hash(&cid).unwrap(),
            None
        ));
        assert_ok!(send_registration(
            &cid,
            initial_registration_action(),
            PublicManage::registration_state_hash(&cid).unwrap(),
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
            PublicManage::registration_state_hash(&cid).unwrap(),
            None
        ));
        let code = pallet::Institutions::<Test>::get(&cid)
            .unwrap()
            .institution_code;
        assert_eq!(
            publicadmin::Pallet::<Test>::resolve_admin_account(code, &cid, &admin(91)),
            Some(admin(91))
        );
        assert_eq!(
            publicadmin::Pallet::<Test>::resolve_admin_account(code, &cid, &admin(1)),
            None
        );
        assert_eq!(
            publicadmin::WalletRoleBindings::<Test>::get(&cid, admin(91)),
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
            PublicManage::registration_state_hash(&cid).unwrap(),
            None
        )
        .is_err());
        assert_eq!(
            publicadmin::AdminAccounts::<Test>::get(&cid)
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
                PublicManage::registration_state_hash(&cid).unwrap(),
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
        let hash = PublicManage::registration_state_hash(&cid).unwrap();
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
            PublicManage::register_institution_admins(
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
                PublicManage::registration_state_hash(&cid).unwrap(),
                Some(2)
            ),
            Error::<Test>::InvalidAssignmentSource
        );
    });
}

#[test]
fn registration_call_nine_scale_fixture_covers_action_snapshot_and_initial_threshold() {
    use codec::{Decode, Encode};
    let cid = generated_cid("register-codec", "CGOV");
    let action = initial_registration_action();
    let call = crate::Call::<Test>::register_institution_admins {
        cid_number: cid.clone(),
        action: action.clone(),
        actor_cid_number: b"REGISTRY-CID".to_vec(),
        actor_role_code: b"REGISTRY-ROLE".to_vec(),
        expected_state_hash: [7; 32],
        initial_threshold: Some(2),
    };
    let expected = (
        9u8,
        cid.clone(),
        action,
        b"REGISTRY-CID".to_vec(),
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
        b"REGISTRY-CID".to_vec(),
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
        let cid = create_cgov("wallet-self");
        let code = pallet::Institutions::<Test>::get(&cid)
            .unwrap()
            .institution_code;
        let pair = sr25519::Pair::from_seed(&[91; 32]);
        let new_wallet = AccountId32::new(pair.public().0);
        let change = adminbase::InstitutionAdminWalletChange {
            admin_account: admin(0),
            expected_wallet: admin(0),
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
        let mut roster = publicadmin::AdminAccounts::<Test>::get(&cid).unwrap();
        roster
            .admins
            .iter_mut()
            .find(|a| a.account_id == change.expected_wallet)
            .unwrap()
            .account_id = new_wallet.clone();
        let assigned_role = grant_close_role(&cid);
        let role_codes_before =
            <PublicManage as entitybase::InstitutionRoleQuery<AccountId32>>::active_role_codes(
                &cid,
                &admin(0),
            );
        let roles = pallet::InstitutionRoleAssignments::<Test>::iter_prefix(&cid)
            .collect::<Vec<_>>()
            .encode();
        let nonce = pallet::InstitutionRoleNonce::<Test>::get(&cid);
        assert_noop!(
            PublicManage::rotate_institution_admin_wallet(
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
            PublicManage::rotate_institution_admin_wallet(
                RuntimeOrigin::signed(admin(0)),
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
            PublicManage::rotate_institution_admin_wallet(
                RuntimeOrigin::signed(admin(0)),
                cid.clone(),
                change.clone(),
                expiry,
                governance_proof
            ),
            Error::<Test>::InvalidWalletProof
        );
        assert_noop!(
            PublicManage::rotate_institution_admin_wallet(
                RuntimeOrigin::signed(admin(0)),
                cid.clone(),
                change.clone(),
                expiry + 1,
                proof.clone()
            ),
            Error::<Test>::InvalidWalletProof
        );
        assert_ok!(PublicManage::rotate_institution_admin_wallet(
            RuntimeOrigin::signed(admin(0)),
            cid.clone(),
            change.clone(),
            expiry,
            proof.clone()
        ));
        assert_eq!(
            publicadmin::Pallet::<Test>::resolve_admin_account(code, &cid, &new_wallet),
            Some(new_wallet.clone())
        );
        assert_eq!(
            publicadmin::Pallet::<Test>::resolve_admin_account(code, &cid, &admin(0)),
            None
        );
        assert_eq!(
            <PublicManage as entitybase::InstitutionRoleQuery<AccountId32>>::active_role_codes(
                &cid,
                &new_wallet
            ),
            role_codes_before
        );
        assert!(<PublicManage as entitybase::InstitutionRoleQuery<
            AccountId32,
        >>::is_active_assignment(
            &cid, &new_wallet, &assigned_role
        ));
        assert!(!<PublicManage as entitybase::InstitutionRoleQuery<
            AccountId32,
        >>::is_active_assignment(
            &cid, &admin(0), &assigned_role
        ));
        assert_eq!(
            publicadmin::Pallet::<Test>::institution_wallet_state(code, &cid, &admin(0))
                .unwrap()
                .revision,
            1
        );
        assert_eq!(
            publicadmin::AdminAccounts::<Test>::get(&cid).unwrap(),
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
            PublicManage::rotate_institution_admin_wallet(
                RuntimeOrigin::signed(admin(0)),
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
        let cid = create_cgov("wallet-vote");
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
            admin_account: admin(1),
            expected_wallet: admin(1),
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
            PublicManage::propose_institution_governance(
                RuntimeOrigin::signed(admin(9)),
                cid.clone(),
                action.clone(),
                cid.to_vec(),
                role.clone()
            ),
            Error::<Test>::PermissionDenied
        );
        assert_ok!(PublicManage::propose_institution_governance(
            RuntimeOrigin::signed(admin(0)),
            cid.clone(),
            action.clone(),
            cid.to_vec(),
            role.clone()
        ));
        let pid = VotingEngine::next_proposal_id() - 1;
        assert_eq!(
            publicadmin::Pallet::<Test>::current_institution_wallet(
                code,
                &cid,
                &change.admin_account
            ),
            Some(change.expected_wallet.clone())
        );
        assert_ok!(cast_yes_votes(&[admin(1), admin(2)], 2, pid));
        assert_eq!(
            publicadmin::Pallet::<Test>::current_institution_wallet(code, &cid, &change.new_wallet),
            Some(change.new_wallet.clone())
        );
        assert_noop!(
            PublicManage::propose_institution_governance(
                RuntimeOrigin::signed(admin(0)),
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
            PublicManage::registration_state_hash(&cid).unwrap(),
            None
        ));
        assert_eq!(
            publicadmin::AdminAccounts::<Test>::get(&cid)
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
        let cid=create_cgov("legacy-close");let role=grant_close_role(&cid);
        pallet::InstitutionRolePermissions::<Test>::mutate(&cid,&role,|items|{for p in items {p.business_action_id.action_code=3;}});
        let rows=publicadmin::AdminAccounts::<Test>::get(&cid).unwrap().admins.to_vec();
        let mut added=rows.clone();let mut unproved=rows[0].clone();unproved.account_id=admin(99);unproved.cid_number=Default::default();added.push(unproved);
        let actions=vec![entitybase::InstitutionGovernanceAction::ReplaceAdmins{admins:added.clone()},entitybase::InstitutionGovernanceAction::ReplaceAdminsAndMutateRoles{admins:added,role_mutations:vec![entitybase::InstitutionRoleMutation::Rename{role_code:role.to_vec(),role_name:b"must-not-apply".to_vec()}],assignment_changes:vec![],legal_representative_change:None}];
        for action in actions {
            let roster_before=publicadmin::AdminAccounts::<Test>::get(&cid).encode();
            let personnel_before=pallet::InstitutionRoleAssignments::<Test>::iter_prefix(&cid).collect::<Vec<_>>().encode();
            assert_noop!(PublicManage::propose_institution_governance(RuntimeOrigin::signed(admin(0)),cid.clone(),action.clone(),cid.to_vec(),role.clone()),Error::<Test>::InvalidInstitutionGovernanceAction);
            assert_noop!(send_registration(&cid,action.clone(),PublicManage::registration_state_hash(&cid).unwrap(),None),Error::<Test>::InvalidRegistrationAction);
            // 仅在隔离测试中复现已在链上的旧提案；生产入口绝不重新接纳旧动作。
            let stored=entitybase::InstitutionGovernanceProposal{institution_code:pallet::Institutions::<Test>::get(&cid).unwrap().institution_code,cid_number:cid.to_vec(),action};
            let mut data=crate::MODULE_TAG.to_vec();data.push(crate::pallet::ACTION_GOVERNANCE);data.extend(stored.encode());
            let plan=PublicManage::build_institution_vote_plan(&admin(0),&cid,&role,3,&data).unwrap();
            let subject=plan.proposer_subject.clone();
            let pid=<InternalVote as InternalVoteEngine<AccountId32>>::create_institution_admin_change_proposal_with_data(admin(0),stored.institution_code,cid.to_vec(),plan,data).unwrap();
            let snapshot=engingcore::VoterSnapshot::<Test>::get(pid,&subject).unwrap();
            let versions=engingcore::FrozenWalletVersions::<Test>::get(pid,&subject).unwrap();
            for voter in snapshot.iter().filter(|v| **v!=admin(0)) {
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
            assert_eq!(publicadmin::AdminAccounts::<Test>::get(&cid).encode(),roster_before);
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

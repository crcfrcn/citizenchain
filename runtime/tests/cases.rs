//! 验证主Runtime模块集成、公开metadata和创世治理权限。

#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

#[test]
fn engine_core_package_keeps_voting_engine_chain_contract() {
    use codec::Encode;

    let metadata = serde_json::to_value(Runtime::metadata().1).expect("真实Runtime元数据");
    let metadata = &metadata["V14"];
    let pallet = metadata["pallets"]
        .as_array().expect("pallets")
        .iter().find(|pallet| pallet["name"] == "VotingEngine")
        .expect("包名改变不能更改链上VotingEngine名称");
    assert_eq!(pallet["index"], 9);
    assert_eq!(pallet["storage"]["prefix"], "VotingEngine");
    let types = metadata["types"]["types"].as_array().expect("types");
    let call = &types.iter().find(|ty| ty["id"] == pallet["calls"]["ty"])
        .expect("VotingEngine调用类型")["type"];
    assert_eq!(call["path"][0], "engingcore");
    let calls = call["def"]["variant"]["variants"].as_array().expect("calls");
    assert_eq!(calls.iter().map(|call| (
        call["name"].as_str().expect("call name"),
        call["index"].as_u64().expect("call index"),
    )).collect::<Vec<_>>(), [
        ("finalize_proposal", 3),
        ("retry_passed_proposal", 4),
        ("cancel_passed_proposal", 5),
    ]);

    // 固定既有链上调用字节；不使用更名后的类型自行生成期望值。
    assert_eq!(RuntimeCall::VotingEngine(engingcore::Call::finalize_proposal {
        proposal_id: 42 }).encode(), [9, 3, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(RuntimeCall::VotingEngine(engingcore::Call::retry_passed_proposal {
        proposal_id: 42 }).encode(), [9, 4, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(RuntimeCall::VotingEngine(engingcore::Call::cancel_passed_proposal {
        proposal_id: 42, reason: Default::default(),
    }).encode(), [9, 5, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

    // RPC查询键必须继续使用原pallet前缀、存储名称和Blake2_128Concat编码。
    for (name, actual) in [
        ("Proposals", engingcore::Proposals::<Runtime>::hashed_key_for(42u128),
        ),
        ("ProposalVotePlans", engingcore::ProposalVotePlans::<Runtime>::hashed_key_for(42u128),
        ),
    ] {
        let mut expected = sp_io::hashing::twox_128(b"VotingEngine").to_vec();
        expected.extend_from_slice(&sp_io::hashing::twox_128(name.as_bytes()));
        expected.extend_from_slice(&sp_io::hashing::blake2_128(&42u128.to_le_bytes()));
        expected.extend_from_slice(&42u128.to_le_bytes());
        assert_eq!(actual, expected, "{name}");
    }
}

#[test]
fn square_subscription_metadata_uses_canonical_prices() {
    // 从真实 Runtime 的 Subscriptions 存储入口解析类型，防止测试自建同名替身。
    let metadata = serde_json::to_value(Runtime::metadata().1).expect("Runtime metadata 可序列化");
    let metadata = &metadata["V14"];
    let pallet = metadata["pallets"]
        .as_array()
        .expect("pallets")
        .iter()
        .find(|pallet| pallet["name"] == "SquarePost")
        .expect("SquarePost");
    assert_eq!(pallet["index"], 34);
    assert_eq!(pallet["storage"]["prefix"], "SquarePost");
    let entry = pallet["storage"]["entries"]
        .as_array()
        .expect("storage entries")
        .iter()
        .find(|entry| entry["name"] == "Subscriptions")
        .expect("Subscriptions");
    let types = metadata["types"]["types"].as_array().expect("types");
    let resolve =
        |id: &serde_json::Value| &types.iter().find(|ty| ty["id"] == *id).expect("type id")["type"];
    let state = resolve(&entry["ty"]["Map"]["value"]);
    let fields = state["def"]["composite"]["fields"]
        .as_array()
        .expect("SubscriptionState fields");
    assert_eq!(
        fields
            .iter()
            .map(|field| field["name"].as_str().expect("field name"))
            .collect::<Vec<_>>(),
        [
            "plan",
            "started_at",
            "last_charged_at",
            "charge_fen",
            "paid_until",
            "subscription_status",
            "price_fen",
            "suspend_reason"
        ]
    );
    for index in [3, 6] {
        assert_eq!(resolve(&fields[index]["type"])["def"]["primitive"], "u128");
    }
    let event = resolve(&pallet["event"]["ty"]);
    let reconsented = event["def"]["variant"]["variants"]
        .as_array()
        .expect("SquarePost events")
        .iter()
        .find(|event| event["name"] == "SubscriptionReconsented")
        .expect("SubscriptionReconsented");
    assert_eq!(reconsented["fields"][2]["name"], "price_fen");
}

#[test]
fn citizen_election_reserves_index_32_without_business_entries() {
    let metadata = Runtime::metadata_ir();
    let pallet = metadata
        .pallets
        .iter()
        .find(|pallet| pallet.name == "CitizenElection")
        .expect("公民选举占位模块必须注册到 Runtime");

    assert_eq!(pallet.index, 32);
    assert!(pallet.calls.is_none());
    assert!(pallet.event.is_none());
    assert!(pallet.storage.is_none());
}

// 簇 1:Runtime 整体自检(4 个用例)


#[test]
fn fsc_genesis_governance_enables_fcsf_multisig_transfer() {
    use entitybase::{
        BusinessActionId, InstitutionRoleAuthorizationQuery, RolePermissionOperation, RoleSubject,
    };
    use primitives::account_derive::AccountKind;
    use primitives::cid::china::china_zf::{CHINA_ZF, FSC_GENESIS_ADMIN_ACCOUNT};
    use primitives::cid::code::{institution_code_from_cid_number, FSC};
    use primitives::institution_asset::{InstitutionAsset, InstitutionAssetAction};
    use primitives::institution_constraints::{ROLE_CODE_DIRECTOR, ROLE_CODE_LEGAL_REPRESENTATIVE};

    new_test_ext().execute_with(|| {
        let fsc = CHINA_ZF
            .iter()
            .find(|n| institution_code_from_cid_number(n.cid_number) == Some(FSC))
            .expect("FSC in CHINA_ZF");
        let cid: publicentity::pallet::CidNumberOf<Runtime> = fsc
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("FSC CID fits");
        let chengwei = AccountId::new(FSC_GENESIS_ADMIN_ACCOUNT);

        // 1) 两岗任职:LR + 局长均由程伟创世任职(Active)。
        for role in [ROLE_CODE_LEGAL_REPRESENTATIVE, ROLE_CODE_DIRECTOR] {
            let role_code: publicentity::RoleCodeOf =
                role.to_vec().try_into().expect("role code fits");
            let assignments =
                publicentity::InstitutionRoleAssignments::<Runtime>::get(&cid, &role_code);
            assert!(
                assignments.iter().any(|a| a.account_id == chengwei
                    && a.assignment_status == publicentity::InstitutionAssignmentStatus::Active),
                "FSC 岗位应由程伟创世任职"
            );
        }

        // 2) 阈值 = 2(2 席严格过半)。
        assert_eq!(
            publicentity::InstitutionGovernanceThresholds::<Runtime>::get(&cid),
            Some(2)
        );

        // 3) 法定代表人 = 程伟。
        let info = publicentity::Institutions::<Runtime>::get(&cid).expect("FSC institution");
        assert_eq!(
            info.legal_representative
                .expect("FSC 法定代表人已任命")
                .account_id,
            chengwei
        );

        // 4) 守卫:FCSF 只放行多签转账,拒其余动作。
        let fcsf = AccountId::new(
            AccountKind::InstitutionFederalCitizenSecurityFund {
                cid_number: fsc.cid_number.as_bytes(),
            }
            .derive(primitives::core_const::SS58_FORMAT),
        );
        assert!(RuntimeInstitutionAsset::can_spend(
            &fcsf,
            InstitutionAssetAction::MultisigTransferExecute
        ));
        assert!(!RuntimeInstitutionAsset::can_spend(
            &fcsf,
            InstitutionAssetAction::MultisigCloseExecute
        ));

        // 5) 授权:程伟以 LR/局长可 Propose 多签转账;两岗均为 Vote 主体。
        let action = BusinessActionId {
            module_tag: multisig::MODULE_TAG.to_vec(),
            action_code: entitybase::business_action::ACTION_MULTISIG_TRANSFER,
        };
        for role in [ROLE_CODE_LEGAL_REPRESENTATIVE, ROLE_CODE_DIRECTOR] {
            let subject = RoleSubject {
                cid_number: fsc.cid_number.as_bytes().to_vec(),
                role_code: role.to_vec(),
            };
            assert!(PublicManage::is_authorized(
                &chengwei,
                &subject,
                &action,
                RolePermissionOperation::Propose
            ));
            assert!(PublicManage::is_authorized(
                &chengwei,
                &subject,
                &action,
                RolePermissionOperation::Vote
            ));
        }
    });
}

#[test]
fn protected_genesis_institution_can_add_dynamic_role_without_mutating_fixed_roles() {
    use entitybase::{InstitutionRoleMutation, RolePermissionOperation};
    use frame_support::assert_ok;

    new_test_ext().execute_with(|| {
        let nrc = &primitives::cid::china::china_cb::CHINA_CB[0];
        let cid_number: publicentity::pallet::CidNumberOf<Runtime> = nrc
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("NRC CID fits");
        let proposal_id = 9001;
        let dynamic_code = entitybase::generate_dynamic_role_code(
            publicentity::MODULE_TAG,
            cid_number.as_slice(),
            0,
            proposal_id,
        );
        assert_ok!(PublicManage::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: primitives::cid::code::NRC,
                cid_number: cid_number.to_vec(),
                proposal_id,
                role_mutations: vec![InstitutionRoleMutation::Create {
                    role_name: "审计工作人员".as_bytes().to_vec(),
                    term_required: false,
                    permissions: vec![entitybase::RolePermissionSpec {
                        business_action_id: entitybase::BusinessActionId {
                            module_tag: publicentity::MODULE_TAG.to_vec(),
                            action_code: u32::from(publicentity::pallet::ACTION_GOVERNANCE),
                        },
                        operation: RolePermissionOperation::Propose,
                    }],
                    assignments: vec![entitybase::InstitutionAssignmentTarget {
                        account_id: AccountId::new(nrc.admins[0]),
                        term_start: 0,
                        term_end: 0,
                        assignment_source:
                            entitybase::InstitutionAssignmentSource::InstitutionGovernance,
                        assignment_source_ref: b"proposal-9001".to_vec(),
                        assignment_status: entitybase::InstitutionAssignmentStatus::Active,
                    }],
                }],
                assignment_changes: vec![],
                legal_representative_change: None,
                result_source_ref: b"proposal-9001".to_vec(),
            }
        ));

        let dynamic_code: publicentity::RoleCodeOf =
            dynamic_code.try_into().expect("dynamic role code fits");
        assert!(publicentity::InstitutionRoles::<Runtime>::contains_key(
            &cid_number,
            &dynamic_code
        ));
        let fixed_code: publicentity::RoleCodeOf =
            primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                .to_vec()
                .try_into()
                .expect("fixed role code fits");
        assert!(publicentity::InstitutionRoles::<Runtime>::contains_key(
            &cid_number,
            fixed_code
        ));

        let foundation = primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION;
        let foundation_cid: privateentity::pallet::CidNumberOf<Runtime> = foundation
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("foundation CID fits");
        let foundation_proposal_id = 9002;
        let foundation_dynamic_code = entitybase::generate_dynamic_role_code(
            privateentity::MODULE_TAG,
            foundation_cid.as_slice(),
            0,
            foundation_proposal_id,
        );
        assert_ok!(PrivateManage::apply_institution_governance_result(
            entitybase::InstitutionGovernanceResult {
                institution_code: *b"SFGY",
                cid_number: foundation_cid.to_vec(),
                proposal_id: foundation_proposal_id,
                role_mutations: vec![InstitutionRoleMutation::Create {
                    role_name: "安全工作人员".as_bytes().to_vec(),
                    term_required: false,
                    permissions: vec![entitybase::RolePermissionSpec {
                        business_action_id: entitybase::BusinessActionId {
                            module_tag: privateentity::MODULE_TAG.to_vec(),
                            action_code: u32::from(privateentity::pallet::ACTION_GOVERNANCE),
                        },
                        operation: RolePermissionOperation::Vote,
                    }],
                    assignments: vec![entitybase::InstitutionAssignmentTarget {
                        account_id: AccountId::new(
                            primitives::cid::china::citizenchain::CITIZENCHAIN_GENESIS_ADMINS[0]
                                .account_id,
                        ),
                        term_start: 0,
                        term_end: 0,
                        assignment_source:
                            entitybase::InstitutionAssignmentSource::InstitutionGovernance,
                        assignment_source_ref: b"proposal-9002".to_vec(),
                        assignment_status: entitybase::InstitutionAssignmentStatus::Active,
                    }],
                }],
                assignment_changes: vec![],
                legal_representative_change: None,
                result_source_ref: b"proposal-9002".to_vec(),
            }
        ));
        let foundation_dynamic_code: privateentity::RoleCodeOf = foundation_dynamic_code
            .try_into()
            .expect("foundation dynamic role code fits");
        assert!(privateentity::InstitutionRoles::<Runtime>::contains_key(
            &foundation_cid,
            foundation_dynamic_code
        ));
    });
}

#[test]
fn institution_transfer_route_uses_exact_fee_account() {
    use frame_support::BoundedVec;
    use onchain::CallFeeRoute;
    use primitives::cid::china::china_cb::CHINA_CB;

    new_test_ext().execute_with(|| {
        let institution = AccountId::new(CHINA_CB[0].main_account);
        let fee_account = AccountId::new(CHINA_CB[0].fee_account);
        let beneficiary_account_id = AccountId::new([99u8; 32]);
        let call = RuntimeCall::MultisigTransfer(multisig::pallet::Call::propose_transfer {
            actor_cid_number: Some(
                CHINA_CB[0]
                    .cid_number
                    .as_bytes()
                    .to_vec()
                    .try_into()
                    .expect("NRC CID fits"),
            ),
            proposer_role_code: Some(
                primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                    .to_vec()
                    .try_into()
                    .expect("committee role fits"),
            ),
            funding_account_id: institution,
            beneficiary_account_id,
            amount: 10000,
            remark: BoundedVec::default(),
        });
        let signer = AccountId::new(CHINA_CB[0].admins[0]);
        let route = <RuntimeFeeRouter as CallFeeRoute<AccountId, RuntimeCall, Balance>>::fee_route(
            &signer, &call,
        );
        assert_eq!(
            route,
            primitives::fee_policy::FeeRoute::Onchain {
                transaction_amount: 0,
                payer_account_id: fee_account,
            }
        );
    });
}

#[test]
fn nrc_he_fund_transfer_debits_he_and_pays_execution_fee_from_nrc_fee_account() {
    use entitybase::InstitutionMultisigQuery;
    use primitives::account_derive::RESERVED_NAME_HE;
    use primitives::cid::china::china_cb::NRC_HE_ACCOUNT;

    new_test_ext().execute_with(|| {
        let nrc = &CHINA_CB[0];
        let cid: engingcore::types::CidNumber = nrc
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("NRC CID fits");
        let role: engingcore::types::RoleCode =
            primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                .to_vec()
                .try_into()
                .expect("committee role fits");
        let he = AccountId::new(NRC_HE_ACCOUNT);
        let main = AccountId::new(nrc.main_account);
        let fee_account = AccountId::new(nrc.fee_account);
        let receiver = AccountId::new([99; 32]);
        let amount = 100_000;
        let _ = Balances::deposit_creating(&he, amount * 2);
        let _ = Balances::deposit_creating(&fee_account, amount);
        let he_before = Balances::free_balance(&he);
        let main_before = Balances::free_balance(&main);
        let fee_before = Balances::free_balance(&fee_account);
        let receiver_before = Balances::free_balance(&receiver);
        assert_eq!(
            RuntimeInstitutionQuery::lookup_institution_account(cid.as_slice(), RESERVED_NAME_HE),
            Some(he.clone())
        );

        assert_ok!(MultisigTransfer::propose_transfer(
            RuntimeOrigin::signed(AccountId::new(nrc.admins[0])),
            Some(cid),
            Some(role.clone()),
            he.clone(),
            receiver.clone(),
            amount,
            Default::default(),
        ));
        let pid = VotingEngine::next_proposal_id() - 1;
        assert_eq!(Balances::free_balance(&he), he_before);
        for admin in &nrc.admins[1..primitives::count_const::NRC_INTERNAL_THRESHOLD as usize] {
            assert_ok!(InternalVote::cast(
                RuntimeOrigin::signed(AccountId::new(*admin)),
                pid,
                internalvote::InternalVoteTicketClaim::InstitutionRole(role.clone()),
                true,
            ));
        }
        <VotingEngine as frame_support::traits::Hooks<BlockNumber>>::on_initialize(
            System::block_number(),
        );

        let fee = primitives::fee_policy::calculate_onchain_fee(amount);
        let nrc_share = fee * Balance::from(primitives::fee_policy::ONCHAIN_FEE_NRC_PERCENT) / 100;
        assert_eq!(
            VotingEngine::proposals(pid).unwrap().status,
            engingcore::STATUS_EXECUTED
        );
        assert_eq!(Balances::free_balance(&he), he_before - amount);
        assert_eq!(Balances::free_balance(&main), main_before);
        assert_eq!(Balances::free_balance(&receiver), receiver_before + amount);
        assert_eq!(
            Balances::free_balance(&fee_account),
            fee_before - fee + nrc_share
        );
    });
}

#[test]
fn institution_operation_debits_only_exact_fee_account_without_signer_fallback() {
    use frame_support::dispatch::GetDispatchInfo;
    use pallet_transaction_payment::OnChargeTransaction;
    use sp_runtime::transaction_validity::{InvalidTransaction, TransactionValidityError};

    type RuntimeCharge = <Runtime as pallet_transaction_payment::Config>::OnChargeTransaction;

    new_test_ext().execute_with(|| {
        let actor_cid_number: publicentity::CidNumberOf<Runtime> = CHINA_CB[0]
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("NRC CID fits");
        let signer = AccountId::new(CHINA_CB[0].admins[0]);
        let fee_account = AccountId::new(CHINA_CB[0].fee_account);
        let funding_account_id = AccountId::new(CHINA_CB[0].main_account);
        let call = RuntimeCall::MultisigTransfer(multisig::pallet::Call::propose_transfer {
            actor_cid_number: Some(actor_cid_number.clone()),
            proposer_role_code: Some(
                primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                    .to_vec()
                    .try_into()
                    .expect("committee role fits"),
            ),
            funding_account_id,
            beneficiary_account_id: AccountId::new([98u8; 32]),
            amount: 50_000,
            remark: Default::default(),
        });
        let dispatch_info = call.get_dispatch_info();

        let _ = Balances::deposit_creating(&signer, 1_000);
        let _ = Balances::deposit_creating(&fee_account, 1_000);
        let signer_before = Balances::free_balance(&signer);
        let fee_before = Balances::free_balance(&fee_account);

        let liquidity = <RuntimeCharge as OnChargeTransaction<Runtime>>::withdraw_fee(
            &signer,
            &call,
            &dispatch_info,
            primitives::fee_policy::ONCHAIN_MIN_FEE,
            primitives::fee_policy::TRANSACTION_TIP,
        )
        .expect("authorized institution operation must debit its exact fee account");
        assert!(liquidity.is_some());
        assert_eq!(Balances::free_balance(&signer), signer_before);
        assert_eq!(
            Balances::free_balance(&fee_account),
            fee_before - primitives::fee_policy::ONCHAIN_MIN_FEE
        );

        // 删除唯一费用账户映射后必须直接拒绝；即使管理员账户有钱也不允许改扣管理员。
        let fee_name: publicentity::AccountNameOf<Runtime> =
            primitives::account_derive::RESERVED_NAME_FEE
                .to_vec()
                .try_into()
                .expect("fee account name fits");
        publicentity::InstitutionAccounts::<Runtime>::remove(&actor_cid_number, &fee_name);
        publicentity::AccountRegisteredCid::<Runtime>::remove(&fee_account);
        let signer_before_reject = Balances::free_balance(&signer);
        let error = <RuntimeCharge as OnChargeTransaction<Runtime>>::can_withdraw_fee(
            &signer,
            &call,
            &dispatch_info,
            primitives::fee_policy::ONCHAIN_MIN_FEE,
            primitives::fee_policy::TRANSACTION_TIP,
        )
        .expect_err("missing exact fee account must reject instead of falling back");
        assert!(matches!(
            error,
            TransactionValidityError::Invalid(InvalidTransaction::Call)
        ));
        assert_eq!(Balances::free_balance(&signer), signer_before_reject);
    });
}

/// 治理业务 pallet 的 `MODULE_TAG` 必须全局唯一。
///
/// 背景:投票引擎达终态后通过 `InternalVoteResultCallback` tuple 广播到
/// 全部业务 Executor,各 Executor 靠 `ProposalData` 前缀的 MODULE_TAG 互斥
/// 认领自己的提案。若两个模块碰撞,同一个提案可能被两个 Executor 同时执行,
/// 产生数据层异常。本测试在编译时固定捕获。




/// benchmark 模式使用既有回调桩，必须返回 Ignored 且不执行生产发行。
#[cfg(feature = "runtime-benchmarks")]
#[test]
fn joint_vote_benchmark_callback_ignores_execution_without_mutating_issuance() {
    new_test_ext().execute_with(|| {
        let proposal_id = 999_999u128;
        resolutionissue::pallet::VotingProposalCount::<Runtime>::put(1u32);
        let issued_before = resolutionissue::pallet::TotalIssued::<Runtime>::get();

        assert!(matches!(
            RuntimeJointVoteResultCallback::on_joint_vote_finalized(proposal_id, true),
            Ok(engingcore::ProposalExecutionOutcome::Ignored)
        ));
        assert_eq!(
            resolutionissue::pallet::VotingProposalCount::<Runtime>::get(),
            1u32
        );
        assert_eq!(
            resolutionissue::pallet::TotalIssued::<Runtime>::get(),
            issued_before
        );
        assert!(resolutionissue::pallet::Executed::<Runtime>::get(proposal_id).is_none());
    });
}

// 簇 2:装配集成测试(18 个用例)
#[cfg(not(feature = "runtime-benchmarks"))]
#[test]
fn joint_vote_callback_routes_to_resolution_issuance_and_executes() {
    use codec::Encode;
    new_test_ext().execute_with(|| {
        // 统一 ID：proposal_id 即投票引擎 ID，不再有双 ID 映射
        let proposal_id = 99u128;
        let per_recipient_amount = 123u128;
        let allocations: Vec<resolutionissue::proposal::RecipientAmount<AccountId, Balance>> =
            CHINA_CB
                .iter()
                .skip(1)
                .map(|node| resolutionissue::proposal::RecipientAmount {
                    recipient_account_id: AccountId::new(node.main_account),
                    amount: per_recipient_amount,
                })
                .collect();
        let recipient_account_id = allocations
            .first()
            .expect("CHINA_CB has province_name recipients")
            .recipient_account_id
            .clone();
        let recipient_before = Balances::free_balance(&recipient_account_id);
        let total_amount = allocations
            .iter()
            .fold(0u128, |sum, item| sum.saturating_add(item.amount));

        // 测试中直接写入 ProposalData/Owner，生产路径必须走 create_*_with_data 原子入口。
        let data = resolutionissue::proposal::IssuanceProposalData {
            actor_cid_number: CHINA_CB[0]
                .cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("NRC CID fits"),
            proposer_role_code: primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                .to_vec()
                .try_into()
                .expect("committee role fits"),
            proposer_account_id: recipient_account_id.clone(),
            reason: b"runtime-integration".to_vec(),
            total_amount,
            allocations,
        };
        let mut encoded = Vec::from(resolutionissue::MODULE_TAG);
        encoded.extend_from_slice(&data.encode());
        let bounded_data: frame_support::BoundedVec<
            u8,
            <Runtime as engingcore::Config>::MaxProposalDataLen,
        > = encoded.try_into().expect("proposal data bound");
        let owner: frame_support::BoundedVec<
            u8,
            <Runtime as engingcore::Config>::MaxModuleTagLen> = resolutionissue::MODULE_TAG
            .to_vec()
            .try_into()
            .expect("module tag bound");
        engingcore::ProposalData::<Runtime>::insert(proposal_id, bounded_data);
        engingcore::ProposalOwner::<Runtime>::insert(proposal_id, owner);
        engingcore::Proposals::<Runtime>::insert(
            proposal_id,
            engingcore::Proposal {
                kind: engingcore::PROPOSAL_KIND_JOINT,
                stage: engingcore::STAGE_JOINT,
                status: engingcore::STATUS_PASSED,
                internal_code: None,
                actor_cid_number: Some(
                    CHINA_CB[0]
                        .cid_number
                        .as_bytes()
                        .to_vec()
                        .try_into()
                        .expect("NRC CID fits"),
                ),
                execution_account_id: None,
                subject_cid_numbers: Default::default(),
                start: 0u32,
                end: 100u32,
            },
        );

        resolutionissue::pallet::VotingProposalCount::<Runtime>::put(1u32);
        engingcore::CallbackExecutionScopes::<Runtime>::insert(proposal_id, ());
        assert_ok!(RuntimeJointVoteResultCallback::on_joint_vote_finalized(
            proposal_id,
            true
        ));
        engingcore::CallbackExecutionScopes::<Runtime>::remove(proposal_id);

        // 验证 VotingProposalCount 已递减
        assert_eq!(
            resolutionissue::pallet::VotingProposalCount::<Runtime>::get(),
            0u32
        );

        assert!(resolutionissue::pallet::Executed::<Runtime>::get(proposal_id).is_some());
        assert_eq!(
            resolutionissue::pallet::TotalIssued::<Runtime>::get(),
            total_amount
        );
        assert_eq!(
            Balances::free_balance(&recipient_account_id),
            recipient_before.saturating_add(per_recipient_amount)
        );
    });
}

#[test]
fn resolution_destro_internal_vote_flow_executes_destroy_and_reduces_issuance() {
    new_test_ext().execute_with(|| {
        let nrc_institution_account = AccountId::new(CHINA_CB[0].main_account);
        let nrc_account = AccountId::new(CHINA_CB[0].main_account);
        let nrc_fee_account = AccountId::new(CHINA_CB[0].fee_account);
        let initial_balance: Balance = 1_000;
        let destroy_amount: Balance = 100;

        let _ = Balances::deposit_creating(&nrc_account, initial_balance);
        let _ = Balances::deposit_creating(&nrc_fee_account, initial_balance);
        let fee_before = Balances::free_balance(&nrc_fee_account);
        let issuance_before = Balances::total_issuance();

        assert_ok!(ResolutionDestroy::propose_destroy(
            RuntimeOrigin::signed(AccountId::new(CHINA_CB[0].admins[0])),
            CHINA_CB[0]
                .cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("NRC CID fits"),
            primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                .to_vec()
                .try_into()
                .expect("committee role fits"),
            nrc_institution_account,
            destroy_amount,
        ));

        let pid = VotingEngine::next_proposal_id().saturating_sub(1);

        // 提案人 admins[0] 在 propose_destroy 时已自动计一票,从 admins[1] 起补足到阈值 13。
        for i in 1..13 {
            assert_ok!(InternalVote::cast(
                RuntimeOrigin::signed(AccountId::new(CHINA_CB[0].admins[i])),
                pid,
                internalvote::InternalVoteTicketClaim::InstitutionRole(
                    primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                        .to_vec()
                        .try_into()
                        .expect("committee role fits"),
                ),
                true,
            ));
        }
        // 投票判定与业务执行已解耦；当前区块维护钩子消费 PASSED 执行队列。
        let now = System::block_number();
        <VotingEngine as frame_support::traits::Hooks<BlockNumber>>::on_initialize(now);

        // 提案数据由 engingcore 延迟清理，执行后仍保留
        assert!(VotingEngine::get_proposal_data(pid).is_some());

        assert_eq!(
            Balances::free_balance(&nrc_account),
            initial_balance - destroy_amount
        );
        let execution_fee = primitives::fee_policy::calculate_onchain_fee(destroy_amount);
        let nrc_fee_share =
            execution_fee * Balance::from(primitives::fee_policy::ONCHAIN_FEE_NRC_PERCENT) / 100;
        // 测试块没有可识别作者，安全基金账户也未在空测试创世中激活；对应份额按
        // 生产分账规则销毁。NRC 份额回到 NRC 自己的费用账户，不改变总发行量。
        assert_eq!(
            Balances::total_issuance(),
            issuance_before - destroy_amount - (execution_fee - nrc_fee_share),
        );
        assert_eq!(
            Balances::free_balance(&nrc_fee_account),
            fee_before - execution_fee + nrc_fee_share,
        );
    });
}

#[test]
fn runtime_fee_router_covers_free_onchain_vote_institution_and_reject_paths() {
    new_test_ext().execute_with(|| {
        use onchain::CallFeeRoute;

        let who = AccountId::new([1u8; 32]);
        let recipient_account_id = AccountId::new([2u8; 32]);

        let system_call = RuntimeCall::System(frame_system::Call::remark {
            remark: b"x".to_vec(),
        });
        let free = <RuntimeFeeRouter as CallFeeRoute<AccountId, RuntimeCall, Balance>>::fee_route(
            &who,
            &system_call,
        );
        assert_eq!(free, primitives::fee_policy::FeeRoute::Free);

        let transfer_call = RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
            dest: sp_runtime::MultiAddress::Id(recipient_account_id.clone()),
            value: 123,
        });
        let amount = <RuntimeFeeRouter as CallFeeRoute<AccountId, RuntimeCall, Balance>>::fee_route(
            &who,
            &transfer_call,
        );
        assert_eq!(amount, primitives::fee_policy::FeeRoute::Reject);

        let remark =
            frame_support::BoundedVec::<u8, frame_support::traits::ConstU32<99>>::try_from(
                b"ordinary transfer remark".to_vec(),
            )
            .expect("remark should fit");
        let transfer_with_remark_call =
            RuntimeCall::OnchainTransaction(onchain::pallet::Call::transfer_with_remark {
                beneficiary_account_id: recipient_account_id,
                amount: 456,
                remark,
            });
        let amount_with_remark = <RuntimeFeeRouter as CallFeeRoute<
            AccountId,
            RuntimeCall,
            Balance,
        >>::fee_route(&who, &transfer_with_remark_call);
        assert_eq!(
            amount_with_remark,
            primitives::fee_policy::FeeRoute::Onchain {
                transaction_amount: 456,
                payer_account_id: who.clone(),
            }
        );

        let internal_vote_call = RuntimeCall::InternalVote(internalvote::pallet::Call::cast {
            proposal_id: 1,
            ticket_claim: internalvote::InternalVoteTicketClaim::Personal,
            approve: true,
        });
        let vote_kind =
            <RuntimeFeeRouter as CallFeeRoute<AccountId, RuntimeCall, Balance>>::fee_route(
                &who,
                &internal_vote_call,
            );
        assert_eq!(
            vote_kind,
            primitives::fee_policy::FeeRoute::Vote {
                payer_account_id: who.clone()
            }
        );

        let miner = AccountId::new([7u8; 32]);
        let fullnode_call =
            RuntimeCall::FullnodeIssuance(fullnodeissue::pallet::Call::bind_reward_account {
                reward_account_id: AccountId::new([8u8; 32]),
            });
        let fullnode_kind =
            <RuntimeFeeRouter as CallFeeRoute<AccountId, RuntimeCall, Balance>>::fee_route(
                &miner,
                &fullnode_call,
            );
        assert_eq!(
            fullnode_kind,
            primitives::fee_policy::FeeRoute::Onchain {
                transaction_amount: 0,
                payer_account_id: miner,
            }
        );

        let nrc_admin = AccountId::new(CHINA_CB[0].admins[0]);
        let nrc_institution_account = AccountId::new(CHINA_CB[0].main_account);
        let resolution_destro_call =
            RuntimeCall::ResolutionDestroy(destroy::pallet::Call::propose_destroy {
                actor_cid_number: CHINA_CB[0]
                    .cid_number
                    .as_bytes()
                    .to_vec()
                    .try_into()
                    .expect("NRC CID fits"),
                proposer_role_code: primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                    .to_vec()
                    .try_into()
                    .expect("committee role fits"),
                institution_account_id: nrc_institution_account,
                amount: 456,
            });
        let resolution_kind =
            <RuntimeFeeRouter as CallFeeRoute<AccountId, RuntimeCall, Balance>>::fee_route(
                &nrc_admin,
                &resolution_destro_call,
            );
        assert_eq!(
            resolution_kind,
            primitives::fee_policy::FeeRoute::Onchain {
                transaction_amount: 0,
                payer_account_id: AccountId::new(CHINA_CB[0].fee_account),
            }
        );
        let unauthorized =
            <RuntimeFeeRouter as CallFeeRoute<AccountId, RuntimeCall, Balance>>::fee_route(
                &who,
                &resolution_destro_call,
            );
        assert_eq!(unauthorized, primitives::fee_policy::FeeRoute::Reject);

        let issuance_placeholder =
            RuntimeCall::OnchainIssuance(onchainissue::pallet::Call::propose_mint {
                actor_cid_number: CHINA_CB[0]
                    .cid_number
                    .as_bytes()
                    .to_vec()
                    .try_into()
                    .expect("NRC CID fits"),
                actor_role_code: primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                    .to_vec()
                    .try_into()
                    .expect("committee role fits"),
                asset_id: 1,
                to_account_id: AccountId::new([6u8; 32]),
                amount: 100,
            });
        let issuance_placeholder_kind = <RuntimeFeeRouter as CallFeeRoute<
            AccountId,
            RuntimeCall,
            Balance,
        >>::fee_route(&nrc_admin, &issuance_placeholder);
        assert_eq!(
            issuance_placeholder_kind,
            primitives::fee_policy::FeeRoute::Reject
        );

        let clearing_bank = &primitives::cid::china::china_ch::CHINA_CH[0];
        let clearing_admin = AccountId::new(clearing_bank.admins[0]);
        let offchain_call =
            RuntimeCall::OffchainTransaction(offchain::pallet::Call::submit_offchain_batch {
                actor_cid_number: clearing_bank
                    .cid_number
                    .as_bytes()
                    .to_vec()
                    .try_into()
                    .expect("clearing bank CID fits"),
                actor_role_code: primitives::governance_skeleton::ROLE_CODE_DIRECTOR
                    .to_vec()
                    .try_into()
                    .expect("director role fits"),
                institution_account_id: AccountId::new(clearing_bank.main_account),
                batch_seq: 1,
                batch: Default::default(),
                batch_signature: Default::default(),
            });
        let offchain_kind =
            <RuntimeFeeRouter as CallFeeRoute<AccountId, RuntimeCall, Balance>>::fee_route(
                &clearing_admin,
                &offchain_call,
            );
        assert_eq!(
            offchain_kind,
            primitives::fee_policy::FeeRoute::Offchain {
                fee_amount: 0,
                payer_account_id: primitives::fee_policy::OffchainFeePayer::BatchItemPayers,
            }
        );

        let unknown_balances_call =
            RuntimeCall::Balances(pallet_balances::Call::upgrade_accounts {
                who: vec![AccountId::new([9u8; 32])],
            });
        let unknown =
            <RuntimeFeeRouter as CallFeeRoute<AccountId, RuntimeCall, Balance>>::fee_route(
                &who,
                &unknown_balances_call,
            );
        assert_eq!(unknown, primitives::fee_policy::FeeRoute::Reject);
    });
}

#[test]
fn runtime_fee_router_treats_proposals_as_operations_not_votes() {
    new_test_ext().execute_with(|| {
        use onchain::CallFeeRoute;

        let (p1, _) = sr25519::Pair::generate();
        let (p2, _) = sr25519::Pair::generate();
        let signer1 = MultiSigner::from(p1.public());
        let who: AccountId = signer1.into_account();
        let admin2: AccountId = MultiSigner::from(p2.public()).into_account();

        let beneficiary_account_id = AccountId::new([78u8; 32]);
        let admins: personalentity::pallet::AdminsOf<Runtime> = vec![who.clone(), admin2.clone()]
            .into_iter()
            .map(|account_id| adminbase::Admin {
                account_id,
                cid_number: Default::default(),
                family_name: adminbase::FamilyName::truncate_from("管理".as_bytes().to_vec()),
                given_name: adminbase::GivenName::truncate_from("员".as_bytes().to_vec()),
            })
            .collect::<Vec<_>>()
            .try_into()
            .expect("admins should fit");
        // 创建提案是普通操作，只有后续 cast 才是固定 1 元投票。
        let account_name: personalentity::pallet::AccountNameOf<Runtime> = b"runtime-test-personal"
            .to_vec()
            .try_into()
            .expect("account_name should fit");

        let create_call =
            RuntimeCall::PersonalManage(personalentity::pallet::Call::propose_create {
                account_name,
                admins: admins.clone(),
                regular_threshold: 2,
                amount: 1_000,
            });
        let create_kind =
            <RuntimeFeeRouter as CallFeeRoute<AccountId, RuntimeCall, Balance>>::fee_route(
                &who,
                &create_call,
            );
        assert_eq!(
            create_kind,
            primitives::fee_policy::FeeRoute::Onchain {
                transaction_amount: 0,
                payer_account_id: who.clone(),
            }
        );

        let nrc_admin = AccountId::new(CHINA_CB[0].admins[0]);
        let close_call = RuntimeCall::PublicManage(
            publicentity::pallet::Call::propose_close_public_institution {
                actor_cid_number: CHINA_CB[0]
                    .cid_number
                    .as_bytes()
                    .to_vec()
                    .try_into()
                    .expect("NRC CID fits"),
                proposer_role_code: primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                    .to_vec()
                    .try_into()
                    .expect("committee role fits"),
                institution_account_id: AccountId::new(CHINA_CB[0].main_account),
                beneficiary_account_id,
            },
        );
        let close_kind =
            <RuntimeFeeRouter as CallFeeRoute<AccountId, RuntimeCall, Balance>>::fee_route(
                &nrc_admin,
                &close_call,
            );
        assert_eq!(
            close_kind,
            primitives::fee_policy::FeeRoute::Onchain {
                transaction_amount: 0,
                payer_account_id: AccountId::new(CHINA_CB[0].fee_account),
            }
        );

        let institution =
            AccountId::new(primitives::cid::china::china_cb::CHINA_CB[0].main_account);
        let transfer_call =
            RuntimeCall::MultisigTransfer(multisig::pallet::Call::propose_transfer {
                actor_cid_number: Some(
                    CHINA_CB[0]
                        .cid_number
                        .as_bytes()
                        .to_vec()
                        .try_into()
                        .expect("NRC CID fits"),
                ),
                proposer_role_code: Some(
                    primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                        .to_vec()
                        .try_into()
                        .expect("committee role fits"),
                ),
                funding_account_id: institution,
                beneficiary_account_id: AccountId::new([79u8; 32]),
                amount: 88_888,
                remark: frame_support::BoundedVec::default(),
            });
        let transfer_kind =
            <RuntimeFeeRouter as CallFeeRoute<AccountId, RuntimeCall, Balance>>::fee_route(
                &nrc_admin,
                &transfer_call,
            );
        assert_eq!(
            transfer_kind,
            primitives::fee_policy::FeeRoute::Onchain {
                transaction_amount: 0,
                payer_account_id: AccountId::new(CHINA_CB[0].fee_account),
            }
        );
    });
}

#[test]
fn multisig_reserved_checker_rejects_stake_and_fee_accounts() {
    let stake = AccountId::new(primitives::cid::china::china_ch::CHINA_CH[0].stake_account);
    assert!(RuntimeReservedAccountGuard::is_reserved(&stake));

    let fee_account = AccountId::new(primitives::cid::china::china_ch::CHINA_CH[0].fee_account);
    assert!(RuntimeReservedAccountGuard::is_reserved(&fee_account));
}

#[test]
fn runtime_call_filter_blocks_disabled_and_low_level_calls() {
    let stake = AccountId::new(primitives::cid::china::china_ch::CHINA_CH[0].stake_account);
    let dst = AccountId::new([9u8; 32]);

    let blocked_by_id = RuntimeCall::Balances(pallet_balances::Call::force_transfer {
        source: sp_runtime::MultiAddress::Id(stake),
        dest: sp_runtime::MultiAddress::Id(dst.clone()),
        value: 1,
    });
    assert!(!RuntimeCallFilter::contains(&blocked_by_id));

    let stake_raw = primitives::cid::china::china_ch::CHINA_CH[0].stake_account;
    let blocked_by_32 = RuntimeCall::Balances(pallet_balances::Call::force_transfer {
        source: sp_runtime::MultiAddress::Address32(stake_raw),
        dest: sp_runtime::MultiAddress::Id(dst.clone()),
        value: 1,
    });
    assert!(!RuntimeCallFilter::contains(&blocked_by_32));

    let blocked_by_raw = RuntimeCall::Balances(pallet_balances::Call::force_transfer {
        source: sp_runtime::MultiAddress::Raw(stake_raw.to_vec()),
        dest: sp_runtime::MultiAddress::Id(dst.clone()),
        value: 1,
    });
    assert!(!RuntimeCallFilter::contains(&blocked_by_raw));

    let blocked_from_regular_account =
        RuntimeCall::Balances(pallet_balances::Call::force_transfer {
            source: sp_runtime::MultiAddress::Id(AccountId::new([8u8; 32])),
            dest: sp_runtime::MultiAddress::Id(dst),
            value: 1,
        });
    assert!(!RuntimeCallFilter::contains(&blocked_from_regular_account));

    let blocked_force_unreserve = RuntimeCall::Balances(pallet_balances::Call::force_unreserve {
        who: sp_runtime::MultiAddress::Id(AccountId::new(
            primitives::cid::china::china_ch::CHINA_CH[0].stake_account,
        )),
        amount: 1,
    });
    assert!(!RuntimeCallFilter::contains(&blocked_force_unreserve));

    let blocked_force_set_balance =
        RuntimeCall::Balances(pallet_balances::Call::force_set_balance {
            who: sp_runtime::MultiAddress::Id(AccountId::new(
                primitives::cid::china::china_ch::CHINA_CH[0].stake_account,
            )),
            new_free: 1,
        });
    assert!(!RuntimeCallFilter::contains(&blocked_force_set_balance));

    let blocked_transfer_allow_death =
        RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
            dest: sp_runtime::MultiAddress::Id(AccountId::new([7u8; 32])),
            value: 1,
        });
    assert!(!RuntimeCallFilter::contains(&blocked_transfer_allow_death));

    let blocked_transfer_keep_alive =
        RuntimeCall::Balances(pallet_balances::Call::transfer_keep_alive {
            dest: sp_runtime::MultiAddress::Id(AccountId::new([7u8; 32])),
            value: 1,
        });
    assert!(!RuntimeCallFilter::contains(&blocked_transfer_keep_alive));

    let blocked_transfer_all = RuntimeCall::Balances(pallet_balances::Call::transfer_all {
        dest: sp_runtime::MultiAddress::Id(AccountId::new([7u8; 32])),
        keep_alive: true,
    });
    assert!(!RuntimeCallFilter::contains(&blocked_transfer_all));

    let blocked_burn = RuntimeCall::Balances(pallet_balances::Call::burn {
        value: 1,
        keep_alive: true,
    });
    assert!(!RuntimeCallFilter::contains(&blocked_burn));

    let remark = frame_support::BoundedVec::<u8, frame_support::traits::ConstU32<99>>::try_from(
        b"ordinary transfer remark".to_vec(),
    )
    .expect("remark should fit");
    let allowed_onchain_transfer =
        RuntimeCall::OnchainTransaction(onchain::pallet::Call::transfer_with_remark {
            beneficiary_account_id: AccountId::new([7u8; 32]),
            amount: 1,
            remark,
        });
    assert!(RuntimeCallFilter::contains(&allowed_onchain_transfer));

    let disabled_issuance =
        RuntimeCall::OnchainIssuance(onchainissue::pallet::Call::propose_mint {
            actor_cid_number: CHINA_CB[0]
                .cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("NRC CID fits"),
            actor_role_code: primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                .to_vec()
                .try_into()
                .expect("committee role fits"),
            asset_id: 1,
            to_account_id: AccountId::new([6u8; 32]),
            amount: 100,
        });
    assert!(!RuntimeCallFilter::contains(&disabled_issuance));
}

#[test]
fn runtime_call_filter_blocks_every_offchain_transaction_call() {
    let clearing_bank = &primitives::cid::china::china_ch::CHINA_CH[0];
    let actor_cid_number: offchain::InstitutionCidNumber = clearing_bank
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("clearing bank CID fits");
    let actor_role_code: offchain::ActorRoleCode =
        primitives::governance_skeleton::ROLE_CODE_DIRECTOR
            .to_vec()
            .try_into()
            .expect("director role fits");
    let peer_id: offchain::ClearingPeerId = b"12D3KooW12345678901234567890123456789012345678"
        .to_vec()
        .try_into()
        .expect("test PeerId fits");
    let rpc_domain: frame_support::BoundedVec<u8, sp_core::ConstU32<128>> = b"clearing.example"
        .to_vec()
        .try_into()
        .expect("test RPC domain fits");

    // OffchainTransaction 当前整体禁用；调用索引 30-34、40-41、50-52
    // 必须全部经过同一 fail-closed 分支，不能只锁住批次提交入口。
    let disabled_calls = [
        (
            "bind_clearing_bank",
            RuntimeCall::OffchainTransaction(offchain::pallet::Call::bind_clearing_bank {
                bank_cid: actor_cid_number.clone(),
            }),
        ),
        (
            "deposit",
            RuntimeCall::OffchainTransaction(offchain::pallet::Call::deposit { amount: 1 }),
        ),
        (
            "withdraw",
            RuntimeCall::OffchainTransaction(offchain::pallet::Call::withdraw { amount: 1 }),
        ),
        (
            "switch_bank",
            RuntimeCall::OffchainTransaction(offchain::pallet::Call::switch_bank {
                new_bank_cid: actor_cid_number.clone(),
            }),
        ),
        (
            "submit_offchain_batch",
            RuntimeCall::OffchainTransaction(offchain::pallet::Call::submit_offchain_batch {
                actor_cid_number: actor_cid_number.clone(),
                actor_role_code: actor_role_code.clone(),
                institution_account_id: AccountId::new(clearing_bank.main_account),
                batch_seq: 1,
                batch: Default::default(),
                batch_signature: Default::default(),
            }),
        ),
        (
            "propose_l2_fee_rate",
            RuntimeCall::OffchainTransaction(offchain::pallet::Call::propose_l2_fee_rate {
                actor_cid_number: actor_cid_number.clone(),
                actor_role_code: actor_role_code.clone(),
                institution_account_id: AccountId::new(clearing_bank.main_account),
                new_rate_bp: 1,
            }),
        ),
        (
            "set_max_l2_fee_rate",
            RuntimeCall::OffchainTransaction(offchain::pallet::Call::set_max_l2_fee_rate {
                new_max: 1,
            }),
        ),
        (
            "register_clearing_bank",
            RuntimeCall::OffchainTransaction(offchain::pallet::Call::register_clearing_bank {
                actor_cid_number: actor_cid_number.clone(),
                actor_role_code: actor_role_code.clone(),
                peer_id,
                rpc_domain: rpc_domain.clone(),
                rpc_port: 9_944,
            }),
        ),
        (
            "update_clearing_bank_endpoint",
            RuntimeCall::OffchainTransaction(
                offchain::pallet::Call::update_clearing_bank_endpoint {
                    actor_cid_number: actor_cid_number.clone(),
                    actor_role_code: actor_role_code.clone(),
                    new_domain: rpc_domain,
                    new_port: 9_945,
                },
            ),
        ),
        (
            "unregister_clearing_bank",
            RuntimeCall::OffchainTransaction(offchain::pallet::Call::unregister_clearing_bank {
                actor_cid_number,
                actor_role_code,
            }),
        ),
    ];

    for (call_name, call) in disabled_calls {
        assert!(
            !RuntimeCallFilter::contains(&call),
            "OffchainTransaction::{call_name} 在业务未启用时必须被 RuntimeCallFilter 拒绝"
        );
    }
}

#[test]
fn runtime_call_filter_has_no_wildcard_arm() {
    // `RuntimeCallFilter` 必须逐 pallet 显式归类,禁止 `_ =>` 兜底分支。
    // 有了通配分支,新增 pallet 的 extrinsic 会默认放行、不触发编译期
    // non-exhaustive 错误,等于把未审查的调用悄悄暴露给外部。
    // 编译器只能保证"删掉某个 pallet 分支会报错",拦不住有人把 `_` 加回来,
    // 故在此做源码级钉死(与 fee_route 同一策略)。
    let source = include_str!("../src/configs.rs");
    let start = source
        .find("impl Contains<RuntimeCall> for RuntimeCallFilter")
        .expect("RuntimeCallFilter 实现必须存在");
    let body = &source[start..];
    let end = body
        .find("\n}\n")
        .expect("RuntimeCallFilter 实现必须有结束大括号");
    let body = &body[..end];
    assert!(
        !body.contains("_ =>"),
        "RuntimeCallFilter 不得出现 `_ =>` 通配分支;新增 pallet 必须显式归类为放行或拒绝"
    );
}



#[cfg(not(feature = "runtime-benchmarks"))]
#[test]
fn joint_vote_callback_missing_proposal_and_runtime_upgrade_route() {
    new_test_ext().execute_with(|| {
        // 不存在的提案 ID 应返回错误
        assert!(RuntimeJointVoteResultCallback::on_joint_vote_finalized(999_999, true).is_err());

        // 测试中直接写入 engingcore 存储；生产路径必须走 create_*_with_data 原子入口。
        let proposal_id = 7u128;
        let proposer = AccountId::new(CHINA_CB[0].admins[0]);
        let reason: upgrade::pallet::ReasonOf<Runtime> =
            b"upgrade".to_vec().try_into().expect("reason");
        let code: upgrade::pallet::CodeOf<Runtime> = vec![1u8, 2, 3].try_into().expect("code");
        let code_hash = <Runtime as frame_system::Config>::Hashing::hash(code.as_slice());

        let proposal = upgrade::pallet::Proposal::<Runtime> {
            actor_cid_number: CHINA_CB[0]
                .cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("NRC CID fits"),
            actor_role_code: primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                .to_vec()
                .try_into()
                .expect("committee role fits"),
            proposer_account_id: proposer,
            reason,
            code_hash,
            expected_pow_params_hash: Default::default(),
            new_pow_params: Default::default(),
        };
        let mut encoded = Vec::from(upgrade::MODULE_TAG);
        encoded.extend_from_slice(&codec::Encode::encode(&proposal));
        let bounded_data: frame_support::BoundedVec<
            u8,
            <Runtime as engingcore::Config>::MaxProposalDataLen,
        > = encoded.try_into().expect("proposal data bound");
        let owner: frame_support::BoundedVec<
            u8,
            <Runtime as engingcore::Config>::MaxModuleTagLen> = upgrade::MODULE_TAG
            .to_vec()
            .try_into()
            .expect("module tag bound");
        engingcore::ProposalData::<Runtime>::insert(proposal_id, bounded_data);
        engingcore::ProposalOwner::<Runtime>::insert(proposal_id, owner);
        let code_vec = code.into_inner();
        let object_len = u32::try_from(code_vec.len()).expect("runtime code length fits u32");
        let object_hash = <Runtime as frame_system::Config>::Hashing::hash(&code_vec);
        let bounded_object: frame_support::BoundedVec<
            u8,
            <Runtime as engingcore::Config>::MaxProposalObjectLen,
        > = code_vec.try_into().expect("runtime code object bound");
        engingcore::ProposalObject::<Runtime>::insert(proposal_id, bounded_object);
        engingcore::ProposalObjectMeta::<Runtime>::insert(
            proposal_id,
            engingcore::ProposalObjectMetadata {
                kind: upgrade::pallet::PROPOSAL_OBJECT_KIND_RUNTIME_WASM,
                object_len,
                object_hash,
            },
        );
        engingcore::Proposals::<Runtime>::insert(
            proposal_id,
            engingcore::Proposal {
                kind: engingcore::PROPOSAL_KIND_JOINT,
                stage: engingcore::STAGE_JOINT,
                status: engingcore::STATUS_REJECTED,
                internal_code: None,
                actor_cid_number: Some(
                    CHINA_CB[0]
                        .cid_number
                        .as_bytes()
                        .to_vec()
                        .try_into()
                        .expect("NRC CID fits"),
                ),
                execution_account_id: None,
                subject_cid_numbers: Default::default(),
                start: 0u32,
                end: 100u32,
            },
        );

        // 回调拒绝后，业务摘要保持创建时快照，终态由 engingcore 统一维护。
        engingcore::CallbackExecutionScopes::<Runtime>::insert(proposal_id, ());
        let outcome = RuntimeJointVoteResultCallback::on_joint_vote_finalized(proposal_id, false)
            .expect("upgrade callback should succeed");
        engingcore::CallbackExecutionScopes::<Runtime>::remove(proposal_id);
        assert_eq!(outcome, engingcore::ProposalExecutionOutcome::Executed);
        let raw = engingcore::Pallet::<Runtime>::get_proposal_data(proposal_id)
            .expect("proposal data should exist");
        let tag = upgrade::MODULE_TAG;
        assert!(
            raw.len() >= tag.len() && &raw[..tag.len()] == tag,
            "MODULE_TAG mismatch"
        );
        let updated = upgrade::pallet::Proposal::<Runtime>::decode(&mut &raw[tag.len()..])
            .expect("should decode");
        assert_eq!(updated.code_hash, code_hash);
        assert_eq!(
            engingcore::Proposals::<Runtime>::get(proposal_id)
                .expect("engine proposal should exist")
                .status,
            engingcore::STATUS_REJECTED
        );
    });
}

// 公民身份上链统一由 identity 承载：注册局管理员提交交易，公民钱包签名确认。
#[test]
fn runtime_citizen_identity_frg_province_admin_registers_voting_identity() {
    new_test_ext().execute_with(|| {
        let (_, registrar, actor_cid_number, actor_role_code) =
            setup_frg_citizen_identity_admin(b"HU");
        let signer_pair =
            sr25519::Pair::from_string("//citizen-wallet-1", None).expect("signer pair");
        let account_id = AccountId::new(signer_pair.public().0);
        let citizen_cid_number: identity::CidNumberBound =
            real_cid_number("RUNTIME-0001", "CTZN", "1")
                .try_into()
                .expect("cid number should fit");
        let expires_at = cid_authorization_expires_at();
        // 占号先行:身份写入前置(占即绑用户钱包账户 + 用户占号签名)。
        assert_ok!(CitizenIdentity::occupy_cid(
            RuntimeOrigin::signed(registrar.clone()),
            actor_cid_number.clone(),
            actor_role_code.clone(),
            citizen_cid_number.clone(),
            account_id.clone(),
            expires_at,
            sign_cid_occupy(&signer_pair, &citizen_cid_number, &account_id, expires_at,),
        ));
        let payload = build_voting_identity_payload(
            account_id.clone(),
            &real_cid_number("RUNTIME-0001", "CTZN", "1"),
            b"HU",
            b"4301",
            b"4301001",
        );
        let (identity_version, authorization_expires_at, signature) =
            sign_citizen_identity_authorization(
                &signer_pair,
                &payload.cid_number.clone(),
                &payload,
            );

        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(registrar),
            actor_cid_number,
            actor_role_code,
            payload,
            identity_version,
            authorization_expires_at,
            signature,
        ));

        assert!(identity::VotingIdentityByCid::<Runtime>::contains_key(
            &citizen_cid_number
        ));
        assert_eq!(
            CitizenIdentity::citizen_subject(&account_id),
            Some(identity::CitizenSubject {
                cid_number: citizen_cid_number,
                account_id,
            })
        );
        assert_eq!(identity::CountryVotingCount::<Runtime>::get(), 1);
    });
}

#[test]
fn runtime_citizen_identity_frg_admin_cannot_register_other_province() {
    new_test_ext().execute_with(|| {
        let (_, registrar, actor_cid_number, actor_role_code) =
            setup_frg_citizen_identity_admin(b"HU");
        let signer_pair =
            sr25519::Pair::from_string("//citizen-wallet-2", None).expect("signer pair");
        let account_id = AccountId::new(signer_pair.public().0);
        let payload = build_voting_identity_payload(
            account_id,
            &real_cid_number("RUNTIME-0002", "CTZN", "1"),
            b"GD",
            b"4401",
            b"4401001",
        );
        let (identity_version, authorization_expires_at, signature) =
            sign_citizen_identity_authorization(
                &signer_pair,
                &payload.cid_number.clone(),
                &payload,
            );

        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(registrar),
                actor_cid_number,
                actor_role_code,
                payload,
                identity_version,
                authorization_expires_at,
                signature,
            ),
            identity::Error::<Runtime>::UnauthorizedRegistrar
        );
    });
}

#[test]
fn runtime_citizen_identity_reader_reads_voting_and_candidate_identity() {
    new_test_ext().execute_with(|| {
        let (_, registrar, actor_cid_number, actor_role_code) =
            setup_frg_citizen_identity_admin(b"HU");
        let signer_pair =
            sr25519::Pair::from_string("//citizen-wallet-3", None).expect("signer pair");
        let account_id = AccountId::new(signer_pair.public().0);
        let citizen_cid_number: identity::CidNumberBound =
            real_cid_number("RUNTIME-0003", "CTZN", "1")
                .try_into()
                .expect("cid number should fit");
        let expires_at = cid_authorization_expires_at();
        // 占号先行:身份写入前置(占即绑用户钱包账户 + 用户占号签名)。
        assert_ok!(CitizenIdentity::occupy_cid(
            RuntimeOrigin::signed(registrar.clone()),
            actor_cid_number.clone(),
            actor_role_code.clone(),
            citizen_cid_number.clone(),
            account_id.clone(),
            expires_at,
            sign_cid_occupy(&signer_pair, &citizen_cid_number, &account_id, expires_at,),
        ));
        let voting = build_voting_identity_payload(
            account_id.clone(),
            &real_cid_number("RUNTIME-0003", "CTZN", "1"),
            b"HU",
            b"4301",
            b"4301001",
        );
        let candidate = identity::CandidateIdentityPayload {
            voting,
            birth_province_code: test_area_code(b"HU"),
            birth_city_code: test_area_code(b"4301"),
            birth_town_code: test_area_code(b"4301001"),
            family_name: b"Runtime".to_vec().try_into().expect("family name fits"),
            given_name: b"Citizen".to_vec().try_into().expect("given name fits"),
            citizen_sex: identity::CitizenSex::Male,
            birth_date: 20000101,
        };
        let (identity_version, authorization_expires_at, signature) =
            sign_citizen_identity_authorization(
                &signer_pair,
                &candidate.voting.cid_number.clone(),
                &candidate,
            );

        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(registrar),
            actor_cid_number,
            actor_role_code,
            candidate,
            identity_version,
            authorization_expires_at,
            signature,
        ));

        let town_scope = identity::PopulationScope::Town(
            test_area_code(b"HU"),
            test_area_code(b"4301"),
            test_area_code(b"4301001"),
        );
        assert!(RuntimeCitizenIdentityReader::voting_subject(&account_id, &town_scope).is_some());
        assert!(
            RuntimeCitizenIdentityReader::candidate_subject(&account_id, &town_scope).is_some()
        );
        assert_eq!(
            RuntimeCitizenIdentityReader::population_data(&town_scope)
                .expect("runtime population data should be ready")
                .eligible_total,
            1
        );
    });
}

#[test]
fn runtime_registrar_rebind_preserves_candidate_cid_and_enforces_residence_scope() {
    new_test_ext().execute_with(|| {
        let (_, registrar, actor_cid_number, hu_role_code) =
            setup_frg_citizen_identity_admin(b"HU");
        let current_pair =
            sr25519::Pair::from_string("//candidate-current-account", None).expect("current pair");
        let current_account_id = AccountId::new(current_pair.public().0);
        let new_pair =
            sr25519::Pair::from_string("//candidate-new-account", None).expect("new pair");
        let new_account_id = AccountId::new(new_pair.public().0);
        let citizen_cid_number: identity::CidNumberBound =
            real_cid_number("RUNTIME-REBIND-CANDIDATE", "CTZN", "1")
                .try_into()
                .expect("cid number should fit");
        let expires_at = cid_authorization_expires_at();

        assert_ok!(CitizenIdentity::occupy_cid(
            RuntimeOrigin::signed(registrar.clone()),
            actor_cid_number.clone(),
            hu_role_code.clone(),
            citizen_cid_number.clone(),
            current_account_id.clone(),
            expires_at,
            sign_cid_occupy(
                &current_pair,
                &citizen_cid_number,
                &current_account_id,
                expires_at,
            ),
        ));
        let voting = build_voting_identity_payload(
            current_account_id.clone(),
            citizen_cid_number.as_slice(),
            b"HU",
            b"4301",
            b"4301001",
        );
        let candidate = identity::CandidateIdentityPayload {
            voting,
            birth_province_code: test_area_code(b"HU"),
            birth_city_code: test_area_code(b"4301"),
            birth_town_code: test_area_code(b"4301001"),
            family_name: b"Runtime".to_vec().try_into().expect("family name fits"),
            given_name: b"Rebind".to_vec().try_into().expect("given name fits"),
            citizen_sex: identity::CitizenSex::Female,
            birth_date: 20000101,
        };
        let (candidate_identity_version, candidate_expires_at, candidate_signature) =
            sign_citizen_identity_authorization(
                &current_pair,
                &candidate.voting.cid_number.clone(),
                &candidate,
            );
        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(registrar.clone()),
            actor_cid_number.clone(),
            hu_role_code.clone(),
            candidate,
            candidate_identity_version,
            candidate_expires_at,
            candidate_signature,
        ));
        let voting_identity_before =
            identity::VotingIdentityByCid::<Runtime>::get(&citizen_cid_number)
                .expect("voting identity exists");
        let candidate_identity_before =
            identity::CandidateIdentityByCid::<Runtime>::get(&citizen_cid_number)
                .expect("candidate identity exists");
        let new_account_signature = sign_cid_admin_rebind(
            &new_pair,
            &citizen_cid_number,
            &current_account_id,
            &new_account_id,
            1,
            expires_at,
        );

        // 同一 FRG 管理员即使另持异省岗位，也不能越过目标公民居住省辖区。
        let (_, other_registrar, other_actor_cid_number, gd_role_code) =
            setup_frg_citizen_identity_admin(b"GD");
        assert_eq!(other_registrar, registrar);
        assert_noop!(
            CitizenIdentity::admin_rebind_cid_account_id(
                RuntimeOrigin::signed(other_registrar),
                other_actor_cid_number,
                gd_role_code,
                citizen_cid_number.clone(),
                new_account_id.clone(),
                1,
                expires_at,
                new_account_signature.clone(),
            ),
            identity::Error::<Runtime>::UnauthorizedRegistrar
        );
        assert_eq!(
            identity::AccountIdByCid::<Runtime>::get(&citizen_cid_number),
            Some(current_account_id.clone())
        );

        // 对应省 FRG 直接办理换绑；不创建换绑投票，也不要求当前账户签名。
        assert_ok!(CitizenIdentity::admin_rebind_cid_account_id(
            RuntimeOrigin::signed(registrar),
            actor_cid_number,
            hu_role_code,
            citizen_cid_number.clone(),
            new_account_id.clone(),
            1,
            expires_at,
            new_account_signature,
        ));
        assert_eq!(
            identity::AccountIdByCid::<Runtime>::get(&citizen_cid_number),
            Some(new_account_id.clone())
        );
        assert_eq!(
            identity::CidByAccountId::<Runtime>::get(&current_account_id),
            None
        );
        assert_eq!(
            identity::BindingRevisionByCid::<Runtime>::get(&citizen_cid_number),
            Some(2)
        );
        assert_eq!(
            identity::VotingIdentityByCid::<Runtime>::get(&citizen_cid_number),
            Some(voting_identity_before)
        );
        assert_eq!(
            identity::CandidateIdentityByCid::<Runtime>::get(&citizen_cid_number),
            Some(candidate_identity_before)
        );
        let town_scope = identity::PopulationScope::Town(
            test_area_code(b"HU"),
            test_area_code(b"4301"),
            test_area_code(b"4301001"),
        );
        assert!(
            RuntimeCitizenIdentityReader::voting_subject(&current_account_id, &town_scope)
                .is_none()
        );
        assert!(
            RuntimeCitizenIdentityReader::candidate_subject(&current_account_id, &town_scope)
                .is_none()
        );
        assert!(
            RuntimeCitizenIdentityReader::voting_subject(&new_account_id, &town_scope).is_some()
        );
        assert!(
            RuntimeCitizenIdentityReader::candidate_subject(&new_account_id, &town_scope).is_some()
        );
    });
}

fn set_runtime_square_platform_membership(cid_number: identity::CidNumberBound, paid_until: u64) {
    square::Subscriptions::<Runtime>::insert(
        (cid_number, square::IssuerKey::Platform),
        square::SubscriptionState {
            plan: square::SubscriptionPlan::Platform {
                membership_level: square::MembershipLevel::Freedom,
            },
            started_at: 1,
            last_charged_at: 1,
            charge_fen: 1,
            paid_until,
            subscription_status: square::SubscriptionStatus::Active,
            price_fen: 1,
            suspend_reason: None,
        },
    );
}

#[test]
fn runtime_square_post_normal_publish_requires_and_records_active_cid() {
    new_test_ext().execute_with(|| {
        let signer_account_id = AccountId::new([42u8; 32]);
        let cid_number = real_cid_number("SQUARE-ANON-NORMAL", "CTZN", "1");
        let bounded_cid_number: identity::CidNumberBound = cid_number
            .clone()
            .try_into()
            .expect("cid number should fit");
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(signer_account_id.clone()),
            bounded_cid_number.clone(),
        ));
        set_runtime_square_platform_membership(bounded_cid_number.clone(), 1_893_456_000_000);
        assert_ok!(SquarePost::publish_post(
            RuntimeOrigin::signed(signer_account_id.clone()),
            b"sqp_runtime_normal".to_vec(),
            square::SquarePostType::Document,
            [3u8; 32],
            b"sqr_runtime_normal".to_vec(),
        ));

        let stored_post_id: square::PostIdOf<Runtime> = b"sqp_runtime_normal"
            .to_vec()
            .try_into()
            .expect("post id fits");
        let stored = square::SquarePosts::<Runtime>::get(stored_post_id)
            .expect("square post should be indexed");
        assert_eq!(stored.signer_account_id, signer_account_id);
        assert_eq!(stored.cid_number, bounded_cid_number);
        assert_eq!(stored.post_type, square::SquarePostType::Document);
        assert_eq!(stored.post_category, square::SquarePostCategory::Normal);
        assert_eq!(
            square::PublishedPostCountByCid::<Runtime>::get(stored.cid_number),
            1
        );
    });
}

#[test]
fn runtime_square_post_derives_normal_category_for_voting_only_citizen() {
    new_test_ext().execute_with(|| {
        let (_, registrar, actor_cid_number, actor_role_code) =
            setup_frg_citizen_identity_admin(b"HU");
        let signer_pair =
            sr25519::Pair::from_string("//square-voting-only", None).expect("signer pair");
        let account_id = AccountId::new(signer_pair.public().0);
        let cid_number = real_cid_number("SQUARE-VOTER", "CTZN", "1");
        let citizen_cid_number: identity::CidNumberBound = cid_number
            .clone()
            .try_into()
            .expect("cid number should fit");
        let expires_at = cid_authorization_expires_at();
        assert_ok!(CitizenIdentity::occupy_cid(
            RuntimeOrigin::signed(registrar.clone()),
            actor_cid_number.clone(),
            actor_role_code.clone(),
            citizen_cid_number.clone(),
            account_id.clone(),
            expires_at,
            sign_cid_occupy(&signer_pair, &citizen_cid_number, &account_id, expires_at,),
        ));
        let voting = build_voting_identity_payload(
            account_id.clone(),
            &cid_number,
            b"HU",
            b"4301",
            b"4301001",
        );
        let (identity_version, authorization_expires_at, signature) =
            sign_citizen_identity_authorization(&signer_pair, &voting.cid_number.clone(), &voting);
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(registrar),
            actor_cid_number,
            actor_role_code,
            voting,
            identity_version,
            authorization_expires_at,
            signature,
        ));
        set_runtime_square_platform_membership(citizen_cid_number.clone(), 1_893_456_000_000);
        assert_ok!(SquarePost::publish_post(
            RuntimeOrigin::signed(account_id),
            b"sqp_runtime_voting".to_vec(),
            square::SquarePostType::Article,
            [4u8; 32],
            b"sqr_runtime_voting".to_vec(),
        ));
        let post_id: square::PostIdOf<Runtime> = b"sqp_runtime_voting"
            .to_vec()
            .try_into()
            .expect("post id fits");
        let stored =
            square::SquarePosts::<Runtime>::get(post_id).expect("square post should be indexed");
        assert_eq!(stored.post_type, square::SquarePostType::Article);
        assert_eq!(stored.post_category, square::SquarePostCategory::Normal);
    });
}

#[test]
fn runtime_square_post_campaign_records_chain_cid_for_verified_account_id() {
    new_test_ext().execute_with(|| {
        let (_, registrar, actor_cid_number, actor_role_code) =
            setup_frg_citizen_identity_admin(b"HU");
        let signer_pair =
            sr25519::Pair::from_string("//square-citizen-wallet", None).expect("signer pair");
        let account_id = AccountId::new(signer_pair.public().0);
        let cid_number = real_cid_number("SQUARE-0001", "CTZN", "1");
        let citizen_cid_number: identity::CidNumberBound = cid_number
            .clone()
            .try_into()
            .expect("cid number should fit");
        let expires_at = cid_authorization_expires_at();

        assert_ok!(CitizenIdentity::occupy_cid(
            RuntimeOrigin::signed(registrar.clone()),
            actor_cid_number.clone(),
            actor_role_code.clone(),
            citizen_cid_number.clone(),
            account_id.clone(),
            expires_at,
            sign_cid_occupy(&signer_pair, &citizen_cid_number, &account_id, expires_at,),
        ));
        let voting = build_voting_identity_payload(
            account_id.clone(),
            &cid_number,
            b"HU",
            b"4301",
            b"4301001",
        );
        let candidate = identity::CandidateIdentityPayload {
            voting,
            birth_province_code: test_area_code(b"HU"),
            birth_city_code: test_area_code(b"4301"),
            birth_town_code: test_area_code(b"4301001"),
            family_name: b"Square".to_vec().try_into().expect("family name fits"),
            given_name: b"Candidate".to_vec().try_into().expect("given name fits"),
            citizen_sex: identity::CitizenSex::Female,
            birth_date: 20000101,
        };
        let (identity_version, authorization_expires_at, signature) =
            sign_citizen_identity_authorization(
                &signer_pair,
                &candidate.voting.cid_number.clone(),
                &candidate,
            );

        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(registrar),
            actor_cid_number,
            actor_role_code,
            candidate,
            identity_version,
            authorization_expires_at,
            signature,
        ));
        set_runtime_square_platform_membership(citizen_cid_number.clone(), 1_893_456_000_000);
        assert_ok!(SquarePost::publish_post(
            RuntimeOrigin::signed(account_id.clone()),
            b"sqp_runtime_campaign_ok".to_vec(),
            square::SquarePostType::Video,
            [5u8; 32],
            b"sqr_runtime_campaign_ok".to_vec(),
        ));

        let stored_post_id: square::PostIdOf<Runtime> = b"sqp_runtime_campaign_ok"
            .to_vec()
            .try_into()
            .expect("post id fits");
        let stored = square::SquarePosts::<Runtime>::get(stored_post_id)
            .expect("square post should be indexed");
        assert_eq!(stored.signer_account_id, account_id);
        assert_eq!(stored.cid_number.to_vec(), cid_number);
        assert_eq!(stored.post_type, square::SquarePostType::Video);
        assert_eq!(stored.post_category, square::SquarePostCategory::Campaign);
        assert_eq!(stored.storage_until, 1_893_456_000_000);
        assert_eq!(
            square::PublishedPostCountByCid::<Runtime>::get(stored.cid_number),
            1
        );
    });
}

#[test]
fn runtime_square_post_fee_kind_uses_onchain_minimum_fee() {
    new_test_ext().execute_with(|| {
        use onchain::CallFeeRoute;

        let who = AccountId::new([44u8; 32]);
        let call = RuntimeCall::SquarePost(square::pallet::Call::publish_post {
            post_id: b"sqp_fee_kind".to_vec(),
            post_type: square::SquarePostType::Document,
            content_hash: [6u8; 32],
            storage_receipt_id: b"sqr_fee_kind".to_vec(),
        });
        let fee_kind =
            <RuntimeFeeRouter as CallFeeRoute<AccountId, RuntimeCall, Balance>>::fee_route(
                &who, &call,
            );
        assert_eq!(
            fee_kind,
            primitives::fee_policy::FeeRoute::Onchain {
                transaction_amount: 0,
                payer_account_id: who,
            }
        );
        assert_eq!(
            primitives::fee_policy::calculate_onchain_fee(0),
            primitives::fee_policy::ONCHAIN_MIN_FEE
        );
        assert_eq!(primitives::fee_policy::ONCHAIN_MIN_FEE, 10);
        // 广场发布费不得改变实际投票的统一费用。
        assert_eq!(primitives::fee_policy::FLAT_FEE, YUAN);
    });
}

#[test]
fn ensure_nrc_admin_and_runtime_admin_directory_paths() {
    new_test_ext().execute_with(|| {
        let nrc_cid: publicentity::pallet::CidNumberOf<Runtime> = CHINA_CB[0]
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("NRC CID fits");
        let nrc_admin = AccountId::new(CHINA_CB[0].admins[0]);
        let outsider = AccountId::new([99u8; 32]);

        let ok_origin = RuntimeOrigin::signed(nrc_admin.clone());
        assert!(<EnsureNrcAdmin as EnsureOrigin<RuntimeOrigin>>::try_origin(ok_origin).is_ok());
        let bad_origin = RuntimeOrigin::signed(outsider.clone());
        assert!(<EnsureNrcAdmin as EnsureOrigin<RuntimeOrigin>>::try_origin(bad_origin).is_err());

        publicadmin::pallet::AdminAccounts::<Runtime>::remove(&nrc_cid);
        assert!(!is_nrc_admin(&nrc_admin));
        assert!(!is_nrc_admin(&outsider));
        assert!(!RuntimeInternalAdminProvider::is_institution_admin(
            engingcore::types::NRC,
            nrc_cid.as_slice(),
            &nrc_admin
        ));
    });
}

// 机构自定义账户关闭已改为「机构在册管理员直接冷签 propose_close(不含凭证)」,由 pallet 在
// origin 处以 `is_institution_admin` 鉴权;该授权属性的回归覆盖在 publicentity/privateentity
// 的 `close_requires_matching_actor_cid_and_an_institution_admin` 等 pallet 级用例。原
// 注册局审批凭证验签(`RuntimeCidInstitutionVerifier`)连同 OnChina 平台签名钥已整体删除。
#[test]
fn onchain_issuance_rejects_admin_without_an_authorized_business_role() {
    new_test_ext().execute_with(|| {
        let nrc = &CHINA_CB[0];
        let actor_cid_number: engingcore::types::CidNumber = nrc
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("NRC CID fits runtime bound");
        let admin = AccountId::new(nrc.admins[0]);
        let main_account = AccountId::new(nrc.main_account);
        // 通用资产发行不是 NRC 委员固定权限；即使账户属于 NRC 且执行账户匹配，
        // 也必须先存在业务模块允许的岗位权限，不能退化为“管理员即可发行”。
        let committee_role: engingcore::types::RoleCode =
            primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                .to_vec()
                .try_into()
                .expect("committee role fits");

        assert_noop!(
            OnchainIssuance::propose_issue(
                RuntimeOrigin::signed(admin.clone()),
                actor_cid_number.clone(),
                committee_role.clone(),
                AccountId::new([250u8; 32]),
                onchainissue::types::AssetClass::Plain,
                b"Institution Asset".to_vec().try_into().expect("name fits"),
                b"IAS".to_vec().try_into().expect("symbol fits"),
                b"plain asset"
                    .to_vec()
                    .try_into()
                    .expect("description fits"),
                2,
                1_000,
            ),
            onchainissue::Error::<Runtime>::ProposeOriginNotAllowed
        );

        assert_noop!(
            OnchainIssuance::propose_issue(
                RuntimeOrigin::signed(admin),
                actor_cid_number,
                committee_role,
                main_account,
                onchainissue::types::AssetClass::Plain,
                b"Institution Asset".to_vec().try_into().expect("name fits"),
                b"IAS".to_vec().try_into().expect("symbol fits"),
                b"plain asset"
                    .to_vec()
                    .try_into()
                    .expect("description fits"),
                2,
                1_000,
            ),
            onchainissue::Error::<Runtime>::ProposeOriginNotAllowed
        );
    });
}

#[test]
fn onchain_issuance_rejects_non_admin_and_non_nrc_monitor_actor() {
    new_test_ext().execute_with(|| {
        let nrc_cid: engingcore::types::CidNumber = CHINA_CB[0]
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("NRC CID fits runtime bound");
        let committee_role: engingcore::types::RoleCode =
            primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                .to_vec()
                .try_into()
                .expect("committee role fits");
        assert_noop!(
            OnchainIssuance::propose_mint(
                RuntimeOrigin::signed(AccountId::new([250u8; 32])),
                nrc_cid,
                committee_role,
                1,
                AccountId::new([1u8; 32]),
                1,
            ),
            onchainissue::Error::<Runtime>::ProposeOriginNotAllowed
        );

        let prc = &CHINA_CB[1];
        let prc_cid: engingcore::types::CidNumber = prc
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("PRC CID fits runtime bound");
        assert_noop!(
            OnchainIssuance::propose_monitor_freeze(
                RuntimeOrigin::signed(AccountId::new(prc.admins[0])),
                prc_cid,
                primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                    .to_vec()
                    .try_into()
                    .expect("committee role fits"),
                1,
                AccountId::new([1u8; 32]),
                [7u8; 32],
            ),
            onchainissue::Error::<Runtime>::InvalidInstitutionContext
        );
    });
}
#[test]
fn city_registry_main_account_can_receive_before_qualifying_and_spend_afterwards() {
    use entitybase::InstitutionMultisigQuery;
    use primitives::account_derive::{RESERVED_NAME_FEE, RESERVED_NAME_MAIN};

    new_test_ext().execute_with(|| {
        let cid = test_cid_number(b"NX013-CREGN-937676656-2026");
        let main = RuntimeInstitutionQuery::lookup_institution_account(&cid, RESERVED_NAME_MAIN)
            .expect("city registry genesis main account");
        let fee = RuntimeInstitutionQuery::lookup_institution_account(&cid, RESERVED_NAME_FEE)
            .expect("city registry genesis fee account");
        let payer = AccountId::new([173; 32]);
        let amount = EXISTENTIAL_DEPOSIT * 2;
        let _ = Balances::deposit_creating(&payer, amount * 4);
        let action = InstitutionAssetAction::MultisigTransferExecute;

        let before = Balances::free_balance(&main);
        assert_ok!(OnchainTransaction::transfer_with_remark(
            RuntimeOrigin::signed(payer.clone()),
            main.clone(),
            amount,
            Default::default(),
        ));
        assert_eq!(Balances::free_balance(&main), before + amount);
        assert!(RuntimeInstitutionAsset::can_spend(&fee, action));
        assert_noop!(
            MultisigTransfer::propose_transfer(
                RuntimeOrigin::signed(payer.clone()),
                Some(cid.clone()),
                None,
                main.clone(),
                payer,
                amount,
                Default::default(),
            ),
            multisig::Error::<Runtime>::InstitutionSpendNotAllowed
        );

        identity::CityRegistryProgress::<Runtime>::mutate(&cid, |progress| {
            progress.registered = 1_001;
            progress.rank = Some(1);
        });
        assert!(RuntimeInstitutionAsset::can_spend(&main, action));
    });
}

// 簇 3:机构资金白名单允许矩阵(4 个用例)
#[test]
fn stake_account_is_completely_blocked() {
    let account = stake_account();
    assert!(!RuntimeInstitutionAsset::can_spend(
        &account,
        InstitutionAssetAction::MultisigTransferExecute
    ));
    assert!(!RuntimeInstitutionAsset::can_spend(
        &account,
        InstitutionAssetAction::MultisigCloseExecute
    ));
    assert!(!RuntimeInstitutionAsset::can_spend(
        &account,
        InstitutionAssetAction::L2ClearingDebit
    ));
    assert!(!RuntimeInstitutionAsset::can_spend(
        &account,
        InstitutionAssetAction::OffchainFeeSweepExecute
    ));
}

#[test]
fn reserved_multisig_only_allows_transfer_and_close() {
    let account = reserved_main_account();
    assert!(RuntimeInstitutionAsset::can_spend(
        &account,
        InstitutionAssetAction::MultisigTransferExecute
    ));
    assert!(RuntimeInstitutionAsset::can_spend(
        &account,
        InstitutionAssetAction::MultisigCloseExecute
    ));
    assert!(!RuntimeInstitutionAsset::can_spend(
        &account,
        InstitutionAssetAction::L2ClearingDebit
    ));
    assert!(!RuntimeInstitutionAsset::can_spend(
        &account,
        InstitutionAssetAction::OffchainFeeSweepExecute
    ));
}

#[test]
fn reserved_fee_account_only_allows_fee_sweep() {
    let account = reserved_fee_account();
    assert!(!RuntimeInstitutionAsset::can_spend(
        &account,
        InstitutionAssetAction::MultisigTransferExecute
    ));
    assert!(!RuntimeInstitutionAsset::can_spend(
        &account,
        InstitutionAssetAction::MultisigCloseExecute
    ));
    assert!(!RuntimeInstitutionAsset::can_spend(
        &account,
        InstitutionAssetAction::L2ClearingDebit
    ));
    assert!(RuntimeInstitutionAsset::can_spend(
        &account,
        InstitutionAssetAction::OffchainFeeSweepExecute
    ));
}

#[test]
fn ordinary_account_allows_all_actions() {
    // 普通账户不命中任何保留/清算谓词,一路放行;is_clearing_account 读 storage,
    // 需在 externalities 内运行。
    new_test_ext().execute_with(|| {
        let account = ordinary_account();
        assert!(RuntimeInstitutionAsset::can_spend(
            &account,
            InstitutionAssetAction::MultisigTransferExecute
        ));
        assert!(RuntimeInstitutionAsset::can_spend(
            &account,
            InstitutionAssetAction::MultisigCloseExecute
        ));
        assert!(RuntimeInstitutionAsset::can_spend(
            &account,
            InstitutionAssetAction::L2ClearingDebit
        ));
        assert!(RuntimeInstitutionAsset::can_spend(
            &account,
            InstitutionAssetAction::OffchainFeeSweepExecute
        ));
    });
}

// 簇 3b:清算账户资金白名单 + 防占号(Step 2)
#[test]
fn clearing_account_only_allows_debit_and_withdraw() {
    new_test_ext().execute_with(|| {
        register_clearing_account();
        let account = clearing_account();
        // 放行:扫码清算扣款 + 用户提现。
        assert!(RuntimeInstitutionAsset::can_spend(
            &account,
            InstitutionAssetAction::L2ClearingDebit
        ));
        assert!(RuntimeInstitutionAsset::can_spend(
            &account,
            InstitutionAssetAction::L3WithdrawOut
        ));
        // 拒绝(漏洞回归):管理员经多签转账 / 关闭 / 机构注资挪用用户存款准备金。
        assert!(!RuntimeInstitutionAsset::can_spend(
            &account,
            InstitutionAssetAction::MultisigTransferExecute
        ));
        assert!(!RuntimeInstitutionAsset::can_spend(
            &account,
            InstitutionAssetAction::MultisigCloseExecute
        ));
        assert!(!RuntimeInstitutionAsset::can_spend(
            &account,
            InstitutionAssetAction::InstitutionCreateFunding
        ));
    });
}

#[test]
fn clearing_account_is_reserved_against_squatting() {
    new_test_ext().execute_with(|| {
        register_clearing_account();
        // 清算账户地址被保留,任何自定义账户注册不得占用。
        assert!(RuntimeReservedAccountGuard::is_reserved(&clearing_account()));
        // 未登记的普通地址不被保留。
        assert!(!RuntimeReservedAccountGuard::is_reserved(
            &ordinary_account()
        ));
    });
}

// ── 创世直铸全量断言(ADR-031 卡3 验收)──

/// 创世直铸当前国家/省/市骨架:常量 296 + 模板派生 49,297 = 49,593,零交易;
/// 镇行政区公权机构不进创世,运行期由注册局按 town_code 注册上链。
/// 并抽查派生首条与链上登记逐字节一致、新补国家机构入链、NJD 创世管理员在位。
#[test]
fn genesis_public_institutions_full_mint_counts() {
    new_test_ext().execute_with(|| {
        let builtin_count = primitives::cid::china::china_cb::CHINA_CB.len()
            + primitives::cid::china::china_ch::CHINA_CH.len()
            + primitives::cid::china::china_zf::CHINA_ZF.len()
            + primitives::cid::china::china_jc::CHINA_JC.len()
            + primitives::cid::china::china_sf::CHINA_SF.len()
            + primitives::cid::china::china_lf::CHINA_LF.len()
            + primitives::cid::china::china_jy::CHINA_JY.len();
        let total = publicentity::Institutions::<Runtime>::iter().count();
        assert_eq!(
            total,
            builtin_count + primitives::cid::official_derive::public_institution_derived_count()
        );
        assert_eq!(builtin_count, 296);
        assert_eq!(total, EXPECTED_PUBLIC_INSTITUTIONS);

        // 49,593 / 99,232 是公权口径：每个机构都有主、费账户，NRC 另有安全基金与和账户，
        // 43 家 PRB 各有质押账户，FSC 另有联邦公民安全基金账户。
        let public_account_count =
            publicentity::InstitutionAccounts::<Runtime>::iter().count();
        assert_eq!(public_account_count, 99_232);

        // 私权创世公民链基金会另占一个机构及主、费两个协议账户；全创世合计为
        // 49,594 个机构、99,234 个机构协议账户，管理员个人钱包不计入机构协议账户。
        let private_institution_count = privateentity::Institutions::<Runtime>::iter().count();
        let private_account_count =
            privateentity::InstitutionAccounts::<Runtime>::iter().count();
        assert_eq!(private_institution_count, 1);
        assert_eq!(private_account_count, 2);
        assert_eq!(total + private_institution_count, 49_594);
        assert_eq!(public_account_count + private_account_count, 99_234);

        // 抽查:派生枚举首条必须与链上登记逐字节一致。
        let mut first: Option<(Vec<u8>, Vec<u8>, Vec<u8>)> = None;
        primitives::cid::official_derive::for_each_public_institution(|cid, full, short| {
            if first.is_none() {
                first = Some((
                    cid.as_bytes().to_vec(),
                    full.as_bytes().to_vec(),
                    short.as_bytes().to_vec(),
                ));
            }
        });
        let (cid, full, short) = first.expect("derived set non-empty");
        let bounded: frame_support::BoundedVec<u8, _> = cid.try_into().expect("cid fits");
        let info = publicentity::Institutions::<Runtime>::get(&bounded).expect("derived minted");
        assert_eq!(info.cid_full_name.to_vec(), full);
        assert_eq!(info.cid_short_name.to_vec(), short);

        // 抽查:本任务新增的国家创世机构必须逐个入链,且机构码与 CID 段一致。
        let mut new_national_count = 0usize;
        for node in primitives::cid::china::china_zf::CHINA_ZF.iter().filter(|node| {
            matches!(
                primitives::cid::code::institution_code_from_cid_number(node.cid_number),
                Some(code)
                    if matches!(
                        primitives::cid::code::institution_code_text(&code),
                        Some("ARM" | "NAV" | "AIR" | "SPF" | "JOS" | "ARC" | "NVC" | "AFC" | "SFC" | "NGB" | "NGC" | "FDA")
                    )
            )
        }) {
            new_national_count += 1;
            let bounded: frame_support::BoundedVec<u8, _> = node
                .cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("new genesis cid fits");
            let info =
                publicentity::Institutions::<Runtime>::get(&bounded).expect("new genesis minted");
            assert_eq!(info.cid_full_name.to_vec(), node.cid_full_name.as_bytes());
            assert_eq!(info.cid_short_name.to_vec(), node.cid_short_name.as_bytes());
            assert_eq!(
                info.institution_code,
                primitives::cid::code::institution_code_from_cid_number(node.cid_number)
                    .expect("new genesis cid code")
            );
            assert!(
                RuntimeReservedAccountGuard::is_reserved(&AccountId::new(node.main_account)),
                "{} 主账户必须进入制度保留地址表",
                node.cid_short_name
            );
            assert!(
                RuntimeReservedAccountGuard::is_reserved(&AccountId::new(node.fee_account)),
                "{} 费用账户必须进入制度保留地址表",
                node.cid_short_name
            );
        }
        assert_eq!(new_national_count, 12);

        // NJD 创世管理员在位(常量特例保留)。
        let njd = primitives::cid::china::china_sf::CHINA_SF
            .iter()
            .find(|n| {
                primitives::cid::code::institution_code_from_cid_number(n.cid_number)
                    == Some(primitives::cid::code::NJD)
            })
            .expect("NJD in china_sf");
        let njd_cid: publicentity::pallet::CidNumberOf<Runtime> = njd
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("NJD CID fits");
        assert!(publicadmin::AdminAccounts::<Runtime>::get(njd_cid).is_some());
    });
}

/// 公民链基金会以非营利私权法人创世写入；一名管理员可同时任三个固定岗位。
#[test]
fn genesis_citizenchain_foundation_is_complete_and_protected() {
    new_test_ext().execute_with(|| {
        let foundation = primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION;
        assert_eq!(foundation.cid_full_name, "公民链技术发展基金会");
        assert_eq!(foundation.cid_short_name, "公民链基金会");
        assert_eq!(
            foundation.full_name_en,
            "CitizenChain Technology Development Foundation"
        );
        assert_eq!(
            foundation.short_name_en,
            "CitizenChain Technology Foundation"
        );
        let cid: privateentity::pallet::CidNumberOf<Runtime> = foundation
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("foundation CID fits");
        let info = privateentity::Institutions::<Runtime>::get(&cid)
            .expect("citizenchain foundation genesis institution exists");
        assert_eq!(
            info.cid_full_name.as_slice(),
            foundation.cid_full_name.as_bytes()
        );
        assert_eq!(
            info.cid_short_name.as_slice(),
            foundation.cid_short_name.as_bytes()
        );
        let legal_representative = info
            .legal_representative
            .expect("foundation legal representative exists");
        assert_eq!(legal_representative.family_name.as_slice(), "程".as_bytes());
        assert_eq!(legal_representative.given_name.as_slice(), "伟".as_bytes());
        assert_eq!(
            legal_representative.cid_number.as_slice(),
            "CN220-CTZN2-198805200-2026".as_bytes()
        );
        assert_eq!(
            legal_representative.account_id,
            AccountId::new(
                primitives::cid::china::citizenchain::CITIZENCHAIN_GENESIS_ADMINS[0].account_id,
            )
        );

        let admin_cid: adminbase::AdminCidNumber = foundation
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("admin CID fits");
        let admins = privateadmin::AdminAccounts::<Runtime>::get(admin_cid)
            .expect("foundation admins exist");
        assert_eq!(admins.institution_code, *b"SFGY");
        assert_eq!(admins.admins.len(), 1);
        assert_eq!(
            admins.admins[0].cid_number.as_slice(),
            "CN220-CTZN2-198805200-2026".as_bytes()
        );
        assert_eq!(admins.admins[0].family_name.as_slice(), "程".as_bytes());
        assert_eq!(admins.admins[0].given_name.as_slice(), "伟".as_bytes());

        for (index, fixed_role) in primitives::cid::china::citizenchain::CITIZENCHAIN_FIXED_ROLES
            .iter()
            .enumerate()
        {
            let role_code: privateentity::RoleCodeOf = fixed_role
                .role_code
                .to_vec()
                .try_into()
                .expect("fixed role code fits");
            let role = privateentity::InstitutionRoles::<Runtime>::get(&cid, &role_code)
                .expect("fixed foundation role exists");
            assert_eq!(role.role_name.as_slice(), fixed_role.role_name);
            assert_eq!(role.role_status, entitybase::InstitutionRoleStatus::Active);
            let assignments =
                privateentity::InstitutionRoleAssignments::<Runtime>::get(&cid, &role_code);
            assert_eq!(assignments.len(), 1);
            assert_eq!(
                assignments[0].account_id,
                AccountId::new(
                    primitives::cid::china::citizenchain::CITIZENCHAIN_GENESIS_ASSIGNMENTS[index]
                        .account_id,
                )
            );
            let permissions =
                privateentity::InstitutionRolePermissions::<Runtime>::get(&cid, &role_code);
            let expected = entitybase::fixed_role_permission_specs(
                *b"SFGY",
                cid.as_slice(),
                fixed_role.role_code,
            );
            assert_eq!(permissions.len(), expected.len());
            assert!(permissions.iter().zip(expected).all(|(actual, expected)| {
                actual.role_subject.cid_number == cid
                    && actual.role_subject.role_code == role_code
                    && actual.business_action_id.module_tag.as_slice() == expected.module_tag
                    && actual.business_action_id.action_code == expected.action_code
                    && actual.operation == expected.operation
            }));
        }
        assert_eq!(
            privateentity::InstitutionGovernanceThresholds::<Runtime>::get(&cid),
            Some(2)
        );
        assert!(RuntimeReservedAccountGuard::is_reserved(&AccountId::new(
            foundation.main_account
        )));
        assert!(RuntimeReservedAccountGuard::is_reserved(&AccountId::new(
            foundation.fee_account
        )));
        assert!(!RuntimeInstitutionAsset::can_spend(
            &AccountId::new(foundation.fee_account),
            InstitutionAssetAction::MultisigTransferExecute
        ));
        assert!(RuntimeInstitutionAsset::can_spend(
            &AccountId::new(foundation.fee_account),
            InstitutionAssetAction::OffchainFeeSweepExecute
        ));
        assert_eq!(privateentity::Institutions::<Runtime>::iter().count(), 1);
    });
}

/// 六个国家级单例在 block#0 精确占用约定身份；三个成员机构保持完整的“尚未组成”状态。
#[test]
fn genesis_national_singletons_exist_and_member_bodies_are_unconstituted() {
    new_test_ext().execute_with(|| {
        for singleton in primitives::institution_constraints::singleton_institutions() {
            let cid: publicentity::pallet::CidNumberOf<Runtime> = singleton
                .cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("singleton CID fits");
            let info = publicentity::Institutions::<Runtime>::get(&cid)
                .expect("national singleton exists at block zero");
            assert_eq!(info.institution_code, singleton.code);
            let main_name: publicentity::pallet::AccountNameOf<Runtime> =
                primitives::account_derive::RESERVED_NAME_MAIN
                    .to_vec()
                    .try_into()
                    .expect("main name fits");
            assert_eq!(
                publicentity::InstitutionAccounts::<Runtime>::get(&cid, main_name)
                    .map(|info| info.account_id),
                Some(AccountId::new(singleton.main_account))
            );
            assert!(publicadmin::AdminAccounts::<Runtime>::get(&cid).is_none());
            assert!(publicentity::InstitutionGovernanceThresholds::<Runtime>::get(&cid).is_none());
        }

        for spec in primitives::institution_constraints::member_composition_specs() {
            let cid: publicentity::pallet::CidNumberOf<Runtime> = spec
                .institution
                .cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("member body CID fits");
            let role_code: publicentity::RoleCodeOf = spec
                .role_code
                .to_vec()
                .try_into()
                .expect("member role code fits");
            assert!(publicentity::InstitutionRoles::<Runtime>::get(&cid, &role_code).is_none());
            assert!(
                publicentity::InstitutionRoleAssignments::<Runtime>::get(&cid, &role_code)
                    .is_empty()
            );
            assert!(publicadmin::AdminAccounts::<Runtime>::get(cid).is_none());
        }
    });
}

/// 参议会必须先独立登记 admins，治理结果只能把既有管理员任命到法定岗位。
#[test]
fn national_member_body_first_composition_and_permanent_range_are_enforced() {
    new_test_ext().execute_with(|| {
        let spec = primitives::institution_constraints::member_composition_specs()[0];
        let main = AccountId::new(spec.institution.main_account);
        let cid_number: publicentity::pallet::CidNumberOf<Runtime> = spec
            .institution
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("member body CID fits");
        let members = |count: u32| {
            (0..count)
                .map(|index| {
                    let mut raw = [0u8; 32];
                    raw[..4].copy_from_slice(&index.to_le_bytes());
                    AccountId::new(raw)
                })
                .collect::<Vec<_>>()
        };
        let result = |accounts: Vec<AccountId>| entitybase::InstitutionGovernanceResult {
            institution_code: spec.institution.code,
            cid_number: spec.institution.cid_number.as_bytes().to_vec(),
            proposal_id: 1,
            role_mutations: vec![],
            assignment_changes: vec![entitybase::InstitutionRoleAssignmentChange {
                role_code: spec.role_code.to_vec(),
                assignments: accounts
                    .into_iter()
                    .map(|account_id| entitybase::InstitutionAssignmentTarget {
                        account_id,
                        term_start: 0,
                        term_end: 0,
                        assignment_source: entitybase::InstitutionAssignmentSource::PopularElection,
                        assignment_source_ref: b"national-election".to_vec(),
                        assignment_status: entitybase::InstitutionAssignmentStatus::Active,
                    })
                    .collect(),
            }],
            legal_representative_change: None,
            result_source_ref: b"national-election".to_vec(),
        };

        let established_admins = members(spec.min_members);
        publicadmin::AdminAccounts::<Runtime>::insert(
            cid_number.clone(),
            adminbase::InstitutionAdmins {
                institution_code: spec.institution.code,
                admins: established_admins
                    .iter()
                    .cloned()
                    .map(|account_id| adminbase::Admin {
                        account_id,
                        cid_number: Default::default(),
                        family_name: Default::default(),
                        given_name: Default::default(),
                    })
                    .collect::<Vec<_>>()
                    .try_into()
                    .expect("member body admins fit"),
            },
        );
        complete_test_public_wallets(&cid_number);
        let member_role_code: publicentity::RoleCodeOf = spec
            .role_code
            .to_vec()
            .try_into()
            .expect("member role code fits");
        publicentity::InstitutionRoles::<Runtime>::insert(
            &cid_number,
            &member_role_code,
            entitybase::InstitutionRole {
                cid_number: cid_number.clone(),
                role_code: member_role_code.clone(),
                role_name: spec
                    .role_name
                    .to_vec()
                    .try_into()
                    .expect("member role name fits"),
                term_required: false,
                role_status: entitybase::InstitutionRoleStatus::Active,
            },
        );

        assert_noop!(
            publicentity::Pallet::<Runtime>::apply_institution_governance_result(result(members(
                spec.min_members - 1,
            ))),
            publicentity::Error::<Runtime>::RequiredMemberCountOutOfRange
        );
        assert_ok!(
            publicentity::Pallet::<Runtime>::apply_institution_governance_result(result(
                established_admins.clone(),
            ))
        );
        let account = publicadmin::AdminAccounts::<Runtime>::get(&cid_number)
            .expect("admins remain independently registered");
        assert_eq!(account.admins.len() as u32, spec.min_members);
        publicentity::InstitutionGovernanceThresholds::<Runtime>::insert(
            &cid_number,
            spec.min_members / 2 + 1,
        );
        let vote_plan = engingcore::VotePlanOf::<AccountId>::try_new(
            engingcore::BusinessActionId {
                module_tag: b"test".to_vec().try_into().expect("owner fits"),
                action_code: 0,
            },
            b"test".to_vec().try_into().expect("owner fits"),
            engingcore::AuthorizationSubject::Institution(engingcore::RoleSubject {
                cid_number: cid_number.clone(),
                role_code: member_role_code.clone(),
            }),
            vec![engingcore::AuthorizationSubject::Institution(
                engingcore::RoleSubject {
                    cid_number: cid_number.clone(),
                    role_code: member_role_code.clone(),
                },
            )],
            engingcore::VotingEngineKind::Internal,
            [0u8; 32],
        )
        .expect("valid singleton vote plan");
        let proposal_id = internalvote::Pallet::<Runtime>::do_create_institution_proposal(
            members(1)[0].clone(),
            spec.institution.code,
            spec.institution.cid_number.as_bytes().to_vec(),
            None,
            vec![spec.institution.cid_number.as_bytes().to_vec()],
            &vote_plan,
        )
        .expect("composed singleton can create internal proposal");
        assert_eq!(
            internalvote::InternalThresholdSnapshot::<Runtime>::get(proposal_id),
            Some(spec.min_members / 2 + 1)
        );
        use entitybase::InstitutionMultisigQuery;
        assert_eq!(
            publicentity::Pallet::<Runtime>::lookup_admin_config(&main)
                .expect("composed singleton exposes current institution configuration")
                .threshold,
            spec.min_members / 2 + 1
        );

        assert_noop!(
            publicentity::Pallet::<Runtime>::apply_institution_governance_result(result(members(
                spec.min_members - 1,
            ))),
            publicentity::Error::<Runtime>::RequiredMemberCountOutOfRange
        );
        assert_eq!(
            publicadmin::AdminAccounts::<Runtime>::get(cid_number)
                .expect("failed change rolls back")
                .admins
                .len() as u32,
            spec.min_members
        );
    });
}

/// 法定代表人治理必须支持整体任命/更换与整体解除，不能留下姓名、CID 或账户半字段。
#[test]
fn institution_governance_can_clear_legal_representative_atomically() {
    new_test_ext().execute_with(|| {
        let cid_number = CHINA_CB[0].cid_number.as_bytes().to_vec();
        let institution_code =
            primitives::cid::code::institution_code_from_cid_number(CHINA_CB[0].cid_number)
                .expect("CHINA_CB CID must contain institution code");
        let cid: publicentity::CidNumberOf<Runtime> =
            cid_number.clone().try_into().expect("CID fits");
        let representative_account_id = publicadmin::AdminAccounts::<Runtime>::get(&cid)
            .expect("NRC genesis admins exist")
            .admins[0]
            .account_id
            .clone();
        let result =
            |legal_representative_change, assignments| entitybase::InstitutionGovernanceResult {
                institution_code,
                cid_number: cid_number.clone(),
                proposal_id: 2,
                role_mutations: vec![],
                assignment_changes: vec![entitybase::InstitutionRoleAssignmentChange {
                    role_code: primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE
                        .to_vec(),
                    assignments,
                }],
                legal_representative_change,
                result_source_ref: b"legal-representative-governance".to_vec(),
            };
        let representative_assignment = entitybase::InstitutionAssignmentTarget {
            account_id: representative_account_id.clone(),
            term_start: 0,
            term_end: 0,
            assignment_source: entitybase::InstitutionAssignmentSource::InstitutionGovernance,
            assignment_source_ref: b"legal-representative-governance".to_vec(),
            assignment_status: entitybase::InstitutionAssignmentStatus::Active,
        };

        assert_ok!(
            publicentity::Pallet::<Runtime>::apply_institution_governance_result(result(
                Some(entitybase::InstitutionLegalRepresentativeChange::Set {
                    family_name: "法定".as_bytes().to_vec(),
                    given_name: "代表人".as_bytes().to_vec(),
                    cid_number: b"CITIZEN-LR-001".to_vec(),
                    account_id: representative_account_id.clone(),
                },),
                vec![representative_assignment],
            ))
        );
        let stored =
            publicentity::Institutions::<Runtime>::get(&cid).expect("genesis institution exists");
        let stored_representative = stored
            .legal_representative
            .expect("legal representative should be stored");
        assert_eq!(
            stored_representative.account_id,
            representative_account_id.clone()
        );

        assert_ok!(
            publicentity::Pallet::<Runtime>::apply_institution_governance_result(result(
                Some(entitybase::InstitutionLegalRepresentativeChange::Clear),
                vec![],
            ))
        );
        let cleared = publicentity::Institutions::<Runtime>::get(&cid)
            .expect("genesis institution still exists");
        assert!(cleared.legal_representative.is_none());

        let second_account = publicadmin::AdminAccounts::<Runtime>::get(&cid)
            .expect("NRC genesis admins remain")
            .admins[1]
            .account_id
            .clone();
        let second_assignment = entitybase::InstitutionAssignmentTarget {
            account_id: second_account,
            term_start: 0,
            term_end: 0,
            assignment_source: entitybase::InstitutionAssignmentSource::InstitutionGovernance,
            assignment_source_ref: b"legal-representative-governance".to_vec(),
            assignment_status: entitybase::InstitutionAssignmentStatus::Active,
        };
        assert_noop!(
            publicentity::Pallet::<Runtime>::apply_institution_governance_result(result(
                Some(entitybase::InstitutionLegalRepresentativeChange::Set {
                    family_name: "重复法定".as_bytes().to_vec(),
                    given_name: "代表人".as_bytes().to_vec(),
                    cid_number: b"CITIZEN-LR-002".to_vec(),
                    account_id: representative_account_id,
                },),
                vec![
                    entitybase::InstitutionAssignmentTarget {
                        account_id: publicadmin::AdminAccounts::<Runtime>::get(&cid)
                            .expect("NRC genesis admins remain")
                            .admins[0]
                            .account_id
                            .clone(),
                        term_start: 0,
                        term_end: 0,
                        assignment_source:
                            entitybase::InstitutionAssignmentSource::InstitutionGovernance,
                        assignment_source_ref: b"legal-representative-governance".to_vec(),
                        assignment_status: entitybase::InstitutionAssignmentStatus::Active,
                    },
                    second_assignment,
                ],
            )),
            publicentity::Error::<Runtime>::FixedRoleSeatsMismatch
        );
    });
}

/// 立法院、监察院、总统府必须先独立登记 admins，岗位任职不得反向生成管理员。
#[test]
fn national_singletons_without_member_ranges_can_be_composed_once() {
    new_test_ext().execute_with(|| {
        for singleton in primitives::institution_constraints::singleton_institutions()
            .into_iter()
            .filter(|item| {
                matches!(
                    item.code,
                    primitives::cid::code::NLG
                        | primitives::cid::code::NSP
                        | primitives::cid::code::PRS
                )
            })
        {
            let cid_number: publicentity::pallet::CidNumberOf<Runtime> = singleton
                .cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("singleton CID fits");
            let admins = vec![AccountId::new([91u8; 32]), AccountId::new([92u8; 32])];
            publicadmin::AdminAccounts::<Runtime>::insert(
                cid_number.clone(),
                adminbase::InstitutionAdmins {
                    institution_code: singleton.code,
                    admins: admins
                        .iter()
                        .cloned()
                        .map(|account_id| adminbase::Admin {
                            account_id,
                            cid_number: Default::default(),
                            family_name: Default::default(),
                            given_name: Default::default(),
                        })
                        .collect::<Vec<_>>()
                        .try_into()
                        .expect("singleton admins fit"),
                },
            );
            complete_test_public_wallets(&cid_number);
            let result = entitybase::InstitutionGovernanceResult {
                institution_code: singleton.code,
                cid_number: singleton.cid_number.as_bytes().to_vec(),
                proposal_id: 100,
                role_mutations: vec![entitybase::InstitutionRoleMutation::Create {
                    role_name: "运行期成员".as_bytes().to_vec(),
                    term_required: false,
                    permissions: vec![entitybase::RolePermissionSpec {
                        business_action_id: entitybase::BusinessActionId {
                            module_tag: publicentity::MODULE_TAG.to_vec(),
                            action_code: u32::from(publicentity::pallet::ACTION_GOVERNANCE),
                        },
                        operation: entitybase::RolePermissionOperation::Propose,
                    }],
                    assignments: admins
                        .iter()
                        .cloned()
                        .map(|account_id| entitybase::InstitutionAssignmentTarget {
                            account_id,
                            term_start: 0,
                            term_end: 0,
                            assignment_source:
                                entitybase::InstitutionAssignmentSource::NominationAppointment,
                            assignment_source_ref: b"first-composition".to_vec(),
                            assignment_status: entitybase::InstitutionAssignmentStatus::Active,
                        })
                        .collect(),
                }],
                assignment_changes: vec![],
                legal_representative_change: None,
                result_source_ref: b"first-composition".to_vec(),
            };

            assert_ok!(
                publicentity::Pallet::<Runtime>::apply_institution_governance_result(result)
            );
            let account = publicadmin::AdminAccounts::<Runtime>::get(&cid_number)
                .expect("admins remain independently registered");
            assert_eq!(
                account
                    .admins
                    .iter()
                    .map(|admin| admin.account_id.clone())
                    .collect::<Vec<_>>(),
                admins
            );
            publicentity::InstitutionGovernanceThresholds::<Runtime>::insert(&cid_number, 2);
        }
    });
}

/// 89 个受保护创世机构的岗位、席位和任职必须由 genesis 构建直接写入 entity/admins。
#[test]
fn genesis_fixed_institution_roles_and_assignments_are_complete() {
    new_test_ext().execute_with(|| {
        let role_code = |raw: &[u8]| -> publicentity::RoleCodeOf {
            raw.to_vec().try_into().expect("fixed role code fits")
        };
        let cid = |raw: &str| -> publicentity::pallet::CidNumberOf<Runtime> {
            raw.as_bytes().to_vec().try_into().expect("fixed cid fits")
        };

        // 法定代表人不是创世必填项。固定机构创世时该字段必须保持为空，
        // 不得从管理员首位、机构主账户或其它账户推导占位值。
        for institution in primitives::governance_skeleton::fixed_institutions() {
            let info = publicentity::Institutions::<Runtime>::get(cid(institution.cid_number))
                .expect("fixed genesis institution exists");
            assert!(info.legal_representative.is_none());
        }

        // 国家储委会、省储委会统一为“委员”。
        for node in primitives::cid::china::china_cb::CHINA_CB.iter() {
            let cid_number = cid(node.cid_number);
            let code = role_code(primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER);
            let role = publicentity::InstitutionRoles::<Runtime>::get(&cid_number, &code)
                .expect("committee role exists");
            assert_eq!(role.cid_number, cid_number);
            assert_eq!(role.role_name.as_slice(), "委员".as_bytes());
            assert!(!role.term_required);
            assert_eq!(role.role_status, entitybase::InstitutionRoleStatus::Active);
            let assignments =
                publicentity::InstitutionRoleAssignments::<Runtime>::get(&cid_number, &code);
            assert_eq!(assignments.len(), node.admins.len());
            assert_eq!(
                assignments
                    .iter()
                    .map(|assignment| assignment.account_id.clone())
                    .collect::<Vec<_>>(),
                node.admins
                    .iter()
                    .copied()
                    .map(AccountId::new)
                    .collect::<Vec<_>>()
            );
            assert!(assignments.iter().all(|assignment| {
                assignment.assignment_source == entitybase::InstitutionAssignmentSource::Genesis
                    && assignment.assignment_source_ref.is_empty()
                    && assignment.assignment_status
                        == entitybase::InstitutionAssignmentStatus::Active
                    && assignment.term_start == 0
                    && assignment.term_end == 0
                    && assignment.cid_number == cid_number
                    && assignment.role_code == code
            }));

            let account_id = publicadmin::AdminAccounts::<Runtime>::get(&cid_number)
                .expect("committee admin account exists");
            assert_eq!(
                account_id.institution_code,
                primitives::cid::code::institution_code_from_cid_number(node.cid_number)
                    .expect("committee code")
            );
            assert_eq!(
                account_id
                    .admins
                    .into_inner()
                    .into_iter()
                    .map(|admin| admin.account_id)
                    .collect::<Vec<_>>(),
                node.admins
                    .iter()
                    .copied()
                    .map(AccountId::new)
                    .collect::<Vec<_>>()
            );
        }

        // 省储行为“董事”。
        for node in primitives::cid::china::china_ch::CHINA_CH.iter() {
            let cid_number = cid(node.cid_number);
            let code = role_code(primitives::governance_skeleton::ROLE_CODE_DIRECTOR);
            let role = publicentity::InstitutionRoles::<Runtime>::get(&cid_number, &code)
                .expect("director role exists");
            assert_eq!(role.role_name.as_slice(), "董事".as_bytes());
            assert!(!role.term_required);
            assert_eq!(role.role_status, entitybase::InstitutionRoleStatus::Active);
            let assignments =
                publicentity::InstitutionRoleAssignments::<Runtime>::get(&cid_number, &code);
            assert_eq!(assignments.len(), node.admins.len());
            assert_eq!(
                assignments
                    .iter()
                    .map(|assignment| assignment.account_id.clone())
                    .collect::<Vec<_>>(),
                node.admins
                    .iter()
                    .copied()
                    .map(AccountId::new)
                    .collect::<Vec<_>>()
            );
            assert!(assignments.iter().all(|assignment| {
                assignment.assignment_source == entitybase::InstitutionAssignmentSource::Genesis
                    && assignment.assignment_source_ref.is_empty()
                    && assignment.assignment_status
                        == entitybase::InstitutionAssignmentStatus::Active
                    && assignment.term_start == 0
                    && assignment.term_end == 0
                    && assignment.cid_number == cid_number
                    && assignment.role_code == code
            }));
            let account_id = publicadmin::AdminAccounts::<Runtime>::get(&cid_number)
                .expect("director admin account exists");
            assert_eq!(
                account_id
                    .admins
                    .into_inner()
                    .into_iter()
                    .map(|admin| admin.account_id)
                    .collect::<Vec<_>>(),
                node.admins
                    .iter()
                    .copied()
                    .map(AccountId::new)
                    .collect::<Vec<_>>()
            );
        }

        // 国家司法院固定 7 护宪、1 首席、2 次席、5 大法官。
        let njd = primitives::cid::china::china_sf::CHINA_SF
            .iter()
            .find(|node| {
                primitives::cid::code::institution_code_from_cid_number(node.cid_number)
                    == Some(primitives::cid::code::NJD)
            })
            .expect("NJD genesis node exists");
        let njd_cid = cid(njd.cid_number);
        for (raw_code, seats) in [
            (
                primitives::governance_skeleton::ROLE_CODE_CONSTITUTION_GUARD,
                7usize,
            ),
            (primitives::governance_skeleton::ROLE_CODE_CHIEF_JUSTICE, 1),
            (
                primitives::governance_skeleton::ROLE_CODE_DEPUTY_CHIEF_JUSTICE,
                2,
            ),
            (primitives::governance_skeleton::ROLE_CODE_JUSTICE, 5),
        ] {
            let code = role_code(raw_code);
            let spec =
                primitives::governance_skeleton::fixed_role_specs(primitives::cid::code::NJD)
                    .into_iter()
                    .find(|spec| spec.role_code == raw_code)
                    .expect("NJD role spec exists");
            let role = publicentity::InstitutionRoles::<Runtime>::get(&njd_cid, &code)
                .expect("NJD role exists");
            assert_eq!(role.role_name.as_slice(), spec.role_name);
            assert!(!role.term_required);
            assert_eq!(role.role_status, entitybase::InstitutionRoleStatus::Active);
            let assignments =
                publicentity::InstitutionRoleAssignments::<Runtime>::get(&njd_cid, &code);
            assert_eq!(assignments.len(), seats);
            assert!(assignments.iter().all(|assignment| {
                assignment.assignment_source == entitybase::InstitutionAssignmentSource::Genesis
                    && assignment.assignment_source_ref.is_empty()
                    && assignment.assignment_status
                        == entitybase::InstitutionAssignmentStatus::Active
                    && assignment.term_start == 0
                    && assignment.term_end == 0
                    && assignment.cid_number == njd_cid
                    && assignment.role_code == code
            }));
        }
        let njd_accounts = [
            primitives::governance_skeleton::ROLE_CODE_CONSTITUTION_GUARD,
            primitives::governance_skeleton::ROLE_CODE_CHIEF_JUSTICE,
            primitives::governance_skeleton::ROLE_CODE_DEPUTY_CHIEF_JUSTICE,
            primitives::governance_skeleton::ROLE_CODE_JUSTICE,
        ]
        .into_iter()
        .flat_map(|raw_code| {
            publicentity::InstitutionRoleAssignments::<Runtime>::get(&njd_cid, role_code(raw_code))
                .into_iter()
                .map(|assignment| assignment.account_id)
        })
        .collect::<Vec<_>>();
        assert_eq!(
            njd_accounts,
            primitives::cid::china::china_sf::NATIONAL_JUDICIAL_YUAN_ADMINS
                .iter()
                .copied()
                .map(AccountId::new)
                .collect::<Vec<_>>()
        );

        // 联邦注册局是一个机构、43 个省专员岗位，每岗位固定 5 人。
        let frg = primitives::cid::china::china_zf::CHINA_ZF
            .iter()
            .find(|node| {
                primitives::cid::code::institution_code_from_cid_number(node.cid_number)
                    == Some(primitives::cid::code::FRG)
            })
            .expect("FRG genesis node exists");
        let frg_cid = cid(frg.cid_number);
        let mut frg_assignments = 0usize;
        for (province_index, province) in primitives::cid::code::PROVINCE_CODE_INFOS
            .iter()
            .enumerate()
        {
            let code = role_code(
                &primitives::governance_skeleton::province_commissioner_role_code(
                    province.province_code,
                ),
            );
            let role = publicentity::InstitutionRoles::<Runtime>::get(&frg_cid, &code)
                .expect("FRG province commissioner role exists");
            assert_eq!(
                role.role_name.as_slice(),
                primitives::governance_skeleton::province_commissioner_role_name(
                    province.province_name,
                )
            );
            assert!(!role.term_required);
            assert_eq!(role.role_status, entitybase::InstitutionRoleStatus::Active);
            let assignments =
                publicentity::InstitutionRoleAssignments::<Runtime>::get(&frg_cid, &code);
            assert_eq!(
                assignments.len(),
                primitives::count_const::FRG_PROVINCE_GROUP_ADMIN_COUNT as usize
            );
            let group_size = primitives::count_const::FRG_PROVINCE_GROUP_ADMIN_COUNT as usize;
            let start = province_index * group_size;
            let expected = primitives::cid::china::china_zf::FEDERAL_REGISTRY_ADMINS
                [start..start + group_size]
                .iter()
                .copied()
                .map(AccountId::new)
                .collect::<Vec<_>>();
            assert_eq!(
                assignments
                    .iter()
                    .map(|assignment| assignment.account_id.clone())
                    .collect::<Vec<_>>(),
                expected
            );
            assert!(assignments.iter().all(|assignment| {
                assignment.assignment_source == entitybase::InstitutionAssignmentSource::Genesis
                    && assignment.assignment_source_ref.is_empty()
                    && assignment.assignment_status
                        == entitybase::InstitutionAssignmentStatus::Active
                    && assignment.term_start == 0
                    && assignment.term_end == 0
                    && assignment.cid_number == frg_cid
                    && assignment.role_code == code
            }));
            frg_assignments += assignments.len();
        }
        assert_eq!(
            frg_assignments,
            primitives::cid::china::china_zf::FEDERAL_REGISTRY_ADMINS.len()
        );
        let frg_admin_account =
            publicadmin::AdminAccounts::<Runtime>::get(&frg_cid).expect("FRG admin account exists");
        assert_eq!(frg_admin_account.admins.len(), frg_assignments);
    });
}

/// 固定权限由创世写入；储委会委员可发起并投票划转，协议升级和决议发行仍由省储行仅投票。
#[test]
fn genesis_fixed_role_permissions_match_the_shared_catalog() {
    use entitybase::{
        business_action::{
            ACTION_FEE_SWEEP_TO_MAIN, ACTION_RESOLUTION_ISSUANCE, ACTION_RUNTIME_UPGRADE,
            MODULE_MULTISIG, MODULE_RESOLUTION_ISSUANCE, MODULE_RUNTIME_UPGRADE,
        },
        RolePermissionOperation,
    };

    new_test_ext().execute_with(|| {
        let role_code = |raw: &[u8]| -> publicentity::RoleCodeOf {
            raw.to_vec().try_into().expect("fixed role code fits")
        };
        let cid = |raw: &str| -> publicentity::pallet::CidNumberOf<Runtime> {
            raw.as_bytes().to_vec().try_into().expect("fixed cid fits")
        };
        let has = |permissions: &publicentity::RolePermissionsOf<Runtime>,
                   module_tag: &[u8],
                   action_code: u32,
                   operation: RolePermissionOperation| {
            permissions.iter().any(|permission| {
                permission.business_action_id.module_tag.as_slice() == module_tag
                    && permission.business_action_id.action_code == action_code
                    && permission.operation == operation
            })
        };

        for node in primitives::cid::china::china_cb::CHINA_CB.iter() {
            let cid_number = cid(node.cid_number);
            let code = role_code(primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER);
            let permissions =
                publicentity::InstitutionRolePermissions::<Runtime>::get(&cid_number, &code);
            for operation in [
                RolePermissionOperation::Propose,
                RolePermissionOperation::Vote,
            ] {
                assert!(has(
                    &permissions,
                    MODULE_MULTISIG,
                    ACTION_FEE_SWEEP_TO_MAIN,
                    operation
                ));
            }
            assert!(has(
                &permissions,
                MODULE_RUNTIME_UPGRADE,
                ACTION_RUNTIME_UPGRADE,
                RolePermissionOperation::Propose,
            ));
            assert!(has(
                &permissions,
                MODULE_RUNTIME_UPGRADE,
                ACTION_RUNTIME_UPGRADE,
                RolePermissionOperation::Vote,
            ));
        }

        let prb = &primitives::cid::china::china_ch::CHINA_CH[0];
        let prb_cid = cid(prb.cid_number);
        let director = role_code(primitives::governance_skeleton::ROLE_CODE_DIRECTOR);
        let permissions =
            publicentity::InstitutionRolePermissions::<Runtime>::get(&prb_cid, &director);
        assert!(has(
            &permissions,
            MODULE_RUNTIME_UPGRADE,
            ACTION_RUNTIME_UPGRADE,
            RolePermissionOperation::Vote,
        ));
        assert!(!has(
            &permissions,
            MODULE_RUNTIME_UPGRADE,
            ACTION_RUNTIME_UPGRADE,
            RolePermissionOperation::Propose,
        ));
        assert!(has(
            &permissions,
            MODULE_RESOLUTION_ISSUANCE,
            ACTION_RESOLUTION_ISSUANCE,
            RolePermissionOperation::Vote,
        ));
        assert!(!has(
            &permissions,
            MODULE_RESOLUTION_ISSUANCE,
            ACTION_RESOLUTION_ISSUANCE,
            RolePermissionOperation::Propose,
        ));

        let nrc = &primitives::cid::china::china_cb::CHINA_CB[0];
        let nrc_cid = cid(nrc.cid_number);
        let lr = role_code(primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE);
        assert!(publicentity::InstitutionRolePermissions::<Runtime>::contains_key(&nrc_cid, &lr));
        assert!(publicentity::InstitutionRolePermissions::<Runtime>::get(&nrc_cid, lr).is_empty());
    });
}

#[test]
fn prc_sweep_upgrade_adds_only_missing_permissions_for_all_fixed_provinces() {
    use entitybase::business_action::{ACTION_FEE_SWEEP_TO_MAIN, MODULE_MULTISIG};
    use entitybase::RolePermissionOperation::{Propose, Vote};
    use frame_support::traits::OnRuntimeUpgrade;
    use primitives::cid::code::{institution_code_from_cid_number, PRC};
    type Migrations = <Runtime as frame_system::Config>::SingleBlockMigrations;

    new_test_ext().execute_with(|| {
        let role_code: publicentity::RoleCodeOf =
            primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                .to_vec()
                .try_into()
                .expect("committee role fits");
        let is_sweep = |permission: &publicentity::institution::role::RolePermissionOf<Runtime>| {
            permission.business_action_id.module_tag.as_slice() == MODULE_MULTISIG
                && permission.business_action_id.action_code == ACTION_FEE_SWEEP_TO_MAIN
        };
        let old_rows: Vec<_> = CHINA_CB
            .iter()
            .filter(|institution| {
                institution_code_from_cid_number(institution.cid_number) == Some(PRC)
            })
            .enumerate()
            .map(|(index, institution)| {
                let cid: publicentity::pallet::CidNumberOf<Runtime> = institution
                    .cid_number
                    .as_bytes()
                    .to_vec()
                    .try_into()
                    .expect("PRC CID fits");
                let mut permissions =
                    publicentity::InstitutionRolePermissions::<Runtime>::get(&cid, &role_code);
                // 第一省只缺 Vote，其余省两项均缺，覆盖已有部分权限的状态。
                permissions.retain(|permission| {
                    !is_sweep(permission) || (index == 0 && permission.operation == Propose)
                });
                publicentity::InstitutionRolePermissions::<Runtime>::insert(
                    &cid,
                    &role_code,
                    &permissions,
                );
                (cid, permissions)
            })
            .collect();
        assert_eq!(old_rows.len(), 43);

        let nrc_cid: publicentity::pallet::CidNumberOf<Runtime> = CHINA_CB[0]
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("NRC CID fits");
        let nrc_before =
            publicentity::InstitutionRolePermissions::<Runtime>::get(&nrc_cid, &role_code);
        let (last_cid, last_permissions) = old_rows.last().unwrap();
        publicentity::InstitutionRolePermissions::<Runtime>::remove(last_cid, &role_code);
        Migrations::on_runtime_upgrade();
        let (first_cid, first_permissions) = &old_rows[0];
        assert_eq!(
            &publicentity::InstitutionRolePermissions::<Runtime>::get(first_cid, &role_code),
            first_permissions
        );
        publicentity::InstitutionRolePermissions::<Runtime>::insert(
            last_cid,
            &role_code,
            last_permissions,
        );

        #[cfg(feature = "try-runtime")]
        assert_ok!(Migrations::try_on_runtime_upgrade(true));
        #[cfg(not(feature = "try-runtime"))]
        Migrations::on_runtime_upgrade();
        let migrated_rows: Vec<_> = old_rows
            .iter()
            .map(|(cid, original)| {
                let permissions =
                    publicentity::InstitutionRolePermissions::<Runtime>::get(cid, &role_code);
                let added_count = 2 - original.iter().filter(|entry| is_sweep(entry)).count();
                assert_eq!(permissions.len(), original.len() + added_count);
                assert_eq!(&permissions[..original.len()], original.as_slice());
                for operation in [Propose, Vote] {
                    assert_eq!(
                        permissions
                            .iter()
                            .filter(|entry| is_sweep(entry)
                                && entry.operation == operation
                                && &entry.role_subject.cid_number == cid
                                && entry.role_subject.role_code == role_code)
                            .count(),
                        1
                    );
                }
                permissions
            })
            .collect();
        assert_eq!(
            publicentity::InstitutionRolePermissions::<Runtime>::get(&nrc_cid, &role_code),
            nrc_before
        );

        // SingleBlockMigrations now also schedules the bounded EVM backfill.
        // Its cursor must finish before asserting tuple-wide idempotence.
        use frame_support::migrations::MultiStepMigrator;
        let mut steps = 0;
        while crate::evm_accounts::BackfillEvmAccounts::ongoing() {
            crate::evm_accounts::BackfillEvmAccounts::step();
            steps += 1;
            assert!(steps < 10_000, "migration cursor must advance");
        }
        let root_before = sp_io::storage::root(sp_runtime::StateVersion::V1);
        Migrations::on_runtime_upgrade();
        assert_eq!(
            sp_io::storage::root(sp_runtime::StateVersion::V1),
            root_before
        );
        for ((cid, _), expected) in old_rows.iter().zip(migrated_rows) {
            assert_eq!(
                publicentity::InstitutionRolePermissions::<Runtime>::get(cid, &role_code),
                expected
            );
        }
    });
}

#[test]
fn runtime_prc_sweep_accepts_exactly_111_fen_left_and_rejects_less() {
    assert_eq!(EXISTENTIAL_DEPOSIT, 111);
    for (amount, executes) in [(10, true), (11, false)] {
        new_test_ext().execute_with(|| {
            let prc = &CHINA_CB[1];
            let cid: engingcore::types::CidNumber = prc
                .cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("PRC CID fits");
            let role: engingcore::types::RoleCode =
                primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                    .to_vec()
                    .try_into()
                    .expect("committee role fits");
            let main = AccountId::new(prc.main_account);
            let fee_account = AccountId::new(prc.fee_account);
            let initial_fee = 131;
            let _ = Balances::deposit_creating(&main, EXISTENTIAL_DEPOSIT);
            assert_ok!(Balances::force_set_balance(
                RuntimeOrigin::root(),
                sp_runtime::MultiAddress::Id(fee_account.clone()),
                initial_fee,
            ));
            let main_before = Balances::free_balance(&main);
            assert_ok!(MultisigTransfer::propose_sweep_to_main(
                RuntimeOrigin::signed(AccountId::new(prc.admins[0])),
                cid,
                role.clone(),
                fee_account.clone(),
                amount,
            ));
            let pid = VotingEngine::next_proposal_id() - 1;
            for admin in &prc.admins[1..primitives::count_const::PRC_INTERNAL_THRESHOLD as usize] {
                assert_ok!(InternalVote::cast(
                    RuntimeOrigin::signed(AccountId::new(*admin)),
                    pid,
                    internalvote::InternalVoteTicketClaim::InstitutionRole(role.clone()),
                    true,
                ));
            }
            <VotingEngine as frame_support::traits::Hooks<BlockNumber>>::on_initialize(
                System::block_number(),
            );
            let (status, main_after, fee_after) = if executes {
                (engingcore::STATUS_EXECUTED, main_before + amount, 111)
            } else {
                assert!(engingcore::ProposalExecutionRetryStates::<Runtime>::contains_key(pid));
                (engingcore::STATUS_PASSED, main_before, initial_fee)
            };
            assert_eq!(VotingEngine::proposals(pid).unwrap().status, status);
            assert_eq!(Balances::free_balance(&main), main_after);
            assert_eq!(Balances::free_balance(&fee_account), fee_after);
        });
    }
}

/// runtime 必须把选举结果路由到 publicentity，并在写入前拒绝固定岗位席位漂移。
#[test]
fn runtime_governance_result_router_enforces_fixed_role_seats() {
    new_test_ext().execute_with(|| {
        let njd = primitives::cid::china::china_sf::CHINA_SF
            .iter()
            .find(|node| {
                primitives::cid::code::institution_code_from_cid_number(node.cid_number)
                    == Some(primitives::cid::code::NJD)
            })
            .expect("NJD genesis node exists");
        let result_source_ref = 700u64.encode();
        let njd_cid_number: publicentity::pallet::CidNumberOf<Runtime> = njd
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("NJD cid fits");
        let njd_admins = publicadmin::AdminAccounts::<Runtime>::get(&njd_cid_number)
            .expect("NJD genesis admins exist")
            .admins
            .into_iter()
            .map(|admin| admin.account_id)
            .collect::<Vec<_>>();
        let result = |accounts: Vec<AccountId>| {
            let assignments = accounts
                .into_iter()
                .map(|account_id| entitybase::InstitutionAssignmentTarget {
                    account_id,
                    term_start: 0,
                    term_end: 0,
                    assignment_source:
                        entitybase::InstitutionAssignmentSource::MutualElection,
                    assignment_source_ref: result_source_ref.clone(),
                    assignment_status: entitybase::InstitutionAssignmentStatus::Active,
                })
                .collect();
            entitybase::InstitutionGovernanceResult {
                institution_code: primitives::cid::code::NJD,
                cid_number: njd.cid_number.as_bytes().to_vec(),
                proposal_id: 700,
                role_mutations: vec![],
                assignment_changes: vec![
                    entitybase::InstitutionRoleAssignmentChange {
                        role_code:
                            primitives::governance_skeleton::ROLE_CODE_CONSTITUTION_GUARD
                                .to_vec(),
                        assignments,
                    },
                ],
                legal_representative_change: None,
                result_source_ref: result_source_ref.clone(),
            }
        };

        let mut role_change = result(vec![]);
        role_change.role_mutations = vec![entitybase::InstitutionRoleMutation::Rename {
            role_code: primitives::governance_skeleton::ROLE_CODE_CONSTITUTION_GUARD.to_vec(),
            role_name: "修改固定岗位".as_bytes().to_vec(),
        }];
        assert_noop!(
            <RuntimeInstitutionGovernanceResultHandler as entitybase::InstitutionGovernanceResultHandler<AccountId>>::apply_institution_governance_result(
                role_change
            ),
            publicentity::Error::<Runtime>::FixedRoleDefinitionImmutable
        );

        assert_noop!(
            <RuntimeInstitutionGovernanceResultHandler as entitybase::InstitutionGovernanceResultHandler<AccountId>>::apply_institution_governance_result(
                result(vec![njd_admins[0].clone()])
            ),
            publicentity::Error::<Runtime>::FixedRoleSeatsMismatch
        );

        let mut replacement = njd_admins
            .iter()
            .take(primitives::governance_skeleton::NJD_CONSTITUTION_GUARD_SEATS as usize)
            .cloned()
            .collect::<Vec<_>>();
        replacement.rotate_left(1);
        assert_ok!(
            <RuntimeInstitutionGovernanceResultHandler as entitybase::InstitutionGovernanceResultHandler<AccountId>>::apply_institution_governance_result(
                result(replacement.clone())
            )
        );

        let role_code: publicentity::RoleCodeOf = primitives::governance_skeleton::ROLE_CODE_CONSTITUTION_GUARD
            .to_vec()
            .try_into()
            .expect("NJD role code fits");
        let stored = publicentity::InstitutionRoleAssignments::<Runtime>::get(
            njd_cid_number.clone(),
            role_code,
        );
        assert_eq!(
            stored
                .into_iter()
                .map(|assignment| assignment.account_id)
                .collect::<Vec<_>>(),
            replacement
        );
        assert_eq!(
            publicentity::InstitutionGovernanceThresholds::<Runtime>::get(njd_cid_number),
            Some(primitives::count_const::NJD_INTERNAL_THRESHOLD)
        );
    });
}

/// 第5步真实 Runtime/Executive 验收：测试密钥为公开 Alith，不使用生产账户。
#[path = "ethereum.rs"]
mod ethereum_execution;

// 通过官方 Runtime API 分发入口验收，不复制创世身份校验实现。
fn chain_identity_api_genesis_hash() -> [u8; 32] {
    use codec::Decode;

    let encoded = crate::apis::api::dispatch("ChainIdentityApi_genesis_hash", &[])
        .expect("已注册链身份 Runtime API");
    assert_eq!(encoded.len(), 32, "API 保持既有 32 字节返回编码");
    <[u8; 32]>::decode(&mut &encoded[..]).expect("创世哈希 SCALE 编码有效")
}

#[test]
fn chain_identity_api_returns_actual_hash_without_writing_storage() {
    new_test_ext().execute_with(|| {
        frame_system::BlockHash::<Runtime>::insert(
            0u32,
            sp_core::H256::from(primitives::genesis::GENESIS_HASH),
        );
        let before = sp_io::storage::root(VERSION.state_version());
        // 当前状态根与冻结的创世根不同，合法的历史创世身份仍应通过。
        assert_ne!(before, primitives::genesis::GENESIS_STATE_ROOT.to_vec());
        assert_eq!(
            chain_identity_api_genesis_hash(),
            primitives::genesis::GENESIS_HASH
        );
        assert_eq!(
            chain_identity_api_genesis_hash(),
            primitives::genesis::GENESIS_HASH
        );
        assert_eq!(sp_io::storage::root(VERSION.state_version()), before);
    });
}

#[test]
#[should_panic(expected = "创世区块哈希缺失")]
fn chain_identity_api_rejects_missing_genesis_hash() {
    new_test_ext().execute_with(|| {
        frame_system::BlockHash::<Runtime>::remove(0u32);
        chain_identity_api_genesis_hash();
    });
}

#[test]
#[should_panic(expected = "创世区块哈希不得为零")]
fn chain_identity_api_rejects_zero_genesis_hash() {
    new_test_ext().execute_with(|| {
        frame_system::BlockHash::<Runtime>::insert(0u32, sp_core::H256::zero());
        chain_identity_api_genesis_hash();
    });
}

#[test]
#[should_panic(expected = "创世区块哈希与冻结身份不一致")]
fn chain_identity_api_rejects_wrong_genesis_hash() {
    new_test_ext().execute_with(|| {
        frame_system::BlockHash::<Runtime>::insert(0u32, sp_core::H256::repeat_byte(0x42));
        chain_identity_api_genesis_hash();
    });
}

#[test]
#[should_panic(expected = "创世区块哈希与冻结身份不一致")]
fn chain_identity_api_rechecks_storage_on_every_query() {
    new_test_ext().execute_with(|| {
        frame_system::BlockHash::<Runtime>::insert(
            0u32,
            sp_core::H256::from(primitives::genesis::GENESIS_HASH),
        );
        assert_eq!(
            chain_identity_api_genesis_hash(),
            primitives::genesis::GENESIS_HASH
        );
        // 成功查询后改成错误身份，后续查询必须失败，不能复用常量或缓存结果。
        frame_system::BlockHash::<Runtime>::insert(0u32, sp_core::H256::repeat_byte(0x42));
        chain_identity_api_genesis_hash();
    });
}

#[test]
fn chain_identity_api_keeps_genesis_after_actual_block_hash_pruning() {
    new_test_ext().execute_with(|| {
        frame_system::BlockHash::<Runtime>::insert(
            0u32,
            sp_core::H256::from(primitives::genesis::GENESIS_HASH),
        );
        frame_system::BlockHash::<Runtime>::insert(1u32, sp_core::H256::repeat_byte(0x42));

        // 实际调用 System::finalize：窗口边界保留块 0，下一高度裁剪块 1。
        System::set_block_number(BLOCK_HASH_COUNT + 1);
        let _ = System::finalize();
        assert!(frame_system::BlockHash::<Runtime>::contains_key(0u32));
        assert!(frame_system::BlockHash::<Runtime>::contains_key(1u32));
        assert_eq!(
            chain_identity_api_genesis_hash(),
            primitives::genesis::GENESIS_HASH
        );

        System::set_block_number(BLOCK_HASH_COUNT + 2);
        let _ = System::finalize();
        assert!(!frame_system::BlockHash::<Runtime>::contains_key(1u32));
        assert!(frame_system::BlockHash::<Runtime>::contains_key(0u32));
        assert_eq!(
            chain_identity_api_genesis_hash(),
            primitives::genesis::GENESIS_HASH
        );
    });
}

// 原创世配置回归通过genesis公开接口验收，主Runtime仅负责实际类型装配。
#[cfg(feature = "std")]
// 创世夹具异常必须立即中止测试，断言式解包仅限本测试模块。
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "genesis.rs"]
mod genesis;

#[test]
fn square_subscription_calldata_matches_owner_fixture_and_real_runtime_index() {
    use codec::Encode;
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../misc/square/tests/subscription_scale_vectors.json"
    ))
    .expect("所属模块固定金标");
    let metadata = Runtime::metadata_ir();
    let pallet = metadata
        .pallets
        .iter()
        .find(|p| p.name == "SquarePost")
        .expect("实际SquarePost模块");
    assert_eq!(
        fixture["pallet_index"].as_u64(),
        Some(u64::from(pallet.index))
    );
    let vector = fixture["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "calldata_subscribe_platform_spark")
        .expect("完整调用固定字节");
    let call = RuntimeCall::SquarePost(square::Call::subscribe {
        issuer: square::IssuerKey::Platform,
        plan: square::SubscriptionPlan::Platform {
            membership_level: square::MembershipLevel::Spark,
        },
        expected_price_fen: 5_999_900,
    });
    assert_eq!(hex::encode(call.encode()), vector["hex"].as_str().unwrap());
}


/// 基准专属主体必须通过实际Runtime的FRG岗位及辖区授权，不消费合成权限替身。
#[cfg(feature = "runtime-benchmarks")]
#[test]
fn address_benchmark_authority_uses_real_runtime_roles_and_scope() {
    use address::AddressUpdateAuthority;

    new_test_ext().execute_with(|| {
        let before = sp_io::storage::root(sp_runtime::StateVersion::V1);
        for action in 0..=4 {
            let actor = RuntimeAddressAuthority::benchmark_authority(action)
                .expect("真实Runtime必须准备地址基准主体");
            let cid = core::str::from_utf8(actor.actor_cid_number.as_slice())
                .expect("真实FRG CID必须是UTF-8");
            assert_eq!(
                primitives::cid::code::institution_code_from_cid_number(cid),
                Some(primitives::cid::code::FRG),
            );
            if action == 0 {
                assert!(RuntimeAddressAuthority::can_update_catalog(
                    &actor.caller,
                    actor.actor_cid_number.as_slice(),
                    actor.actor_role_code.as_slice(),
                    action,
                ));
            } else {
                assert!(RuntimeAddressAuthority::can_update_address(
                    &actor.caller,
                    actor.actor_cid_number.as_slice(),
                    actor.actor_role_code.as_slice(),
                    &actor.province_code,
                    &actor.city_code,
                    action,
                ));
                let other = primitives::cid::code::PROVINCE_CODE_INFOS.iter()
                    .find(|entry| entry.province_code.as_slice() != actor.province_code.as_slice())
                    .expect("行政区真源必须提供另一真实省份");
                assert!(!RuntimeAddressAuthority::can_update_address(
                    &actor.caller,
                    actor.actor_cid_number.as_slice(),
                    actor.actor_role_code.as_slice(),
                    other.province_code.as_slice(),
                    &actor.city_code,
                    action,
                ));
            }
        }
        assert!(RuntimeAddressAuthority::benchmark_authority(5).is_none());
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), before);
    });
}

/// 空的临时基准状态只能经既有genesis公开入口补齐，随后仍须通过实际权限。
#[cfg(feature = "runtime-benchmarks")]
#[test]
fn address_benchmark_authority_initializes_missing_runtime_state() {
    use address::AddressUpdateAuthority;

    let storage = frame_system::GenesisConfig::<Runtime>::default()
        .build_storage().expect("系统初始存储必须可构建");
    sp_io::TestExternalities::new(storage).execute_with(|| {
        System::set_block_number(1);
        let actor = RuntimeAddressAuthority::benchmark_authority(3)
            .expect("空的基准状态必须经原初始化入口准备真实岗位");
        assert!(<PublicAdmins as adminbase::InstitutionAdminQuery<AccountId,
        >>::institution_admins_exist(
            primitives::cid::code::FRG,
            actor.actor_cid_number.as_slice(),
        ));
        assert!(RuntimeAddressAuthority::can_update_address(
            &actor.caller,
            actor.actor_cid_number.as_slice(),
            actor.actor_role_code.as_slice(),
            &actor.province_code,
            &actor.city_code,
            3,
        ));
        let before = sp_io::storage::root(sp_runtime::StateVersion::V1);
        assert!(RuntimeAddressAuthority::benchmark_authority(5).is_none());
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), before);
    });
}


#[test]
fn creg_capability_requires_one_existing_public_entity() {
    use entitybase::{
        business_action::*, BusinessActionId, InstitutionCapabilityPolicy, RolePermissionOperation,
    };
    new_test_ext().execute_with(|| {
        let cid = test_cid_number(b"NX013-CREGN-937676656-2026");
        let action = BusinessActionId {
            module_tag: MODULE_CITIZEN_IDENTITY.to_vec(),
            action_code: ACTION_REGISTER_VOTING_IDENTITY,
        };
        assert!(RuntimeInstitutionCapabilityPolicy::allows(
            &cid,
            &action,
            RolePermissionOperation::Propose
        ));
        assert!(!RuntimeInstitutionCapabilityPolicy::allows(
            &cid,
            &action,
            RolePermissionOperation::Vote
        ));
        let info = publicentity::Institutions::<Runtime>::get(&cid).unwrap();
        privateentity::Institutions::<Runtime>::insert(&cid, info);
        assert!(!RuntimeInstitutionCapabilityPolicy::allows(
            &cid,
            &action,
            RolePermissionOperation::Propose
        ));
        publicentity::Institutions::<Runtime>::remove(&cid);
        assert!(!RuntimeInstitutionCapabilityPolicy::allows(
            &cid,
            &action,
            RolePermissionOperation::Propose
        ));
        privateentity::Institutions::<Runtime>::remove(&cid);
        assert!(!RuntimeInstitutionCapabilityPolicy::allows(
            &cid,
            &action,
            RolePermissionOperation::Propose
        ));
    });
}

#[test]
fn creg_business_still_requires_roster_permission_effective_assignment_and_local_scope() {
    use address::AddressUpdateAuthority;
    use entitybase::{
        business_action::*, RoleBusinessPermission, RolePermissionOperation, RoleSubject,
    };
    use identity::CitizenIdentityAuthority;
    new_test_ext().execute_with(|| {
        let cid = test_cid_number(b"NX013-CREGN-937676656-2026");
        let who = AccountId::new([61; 32]);
        // 仅准备下游授权夹具；本步不实现或绕过管理员登记接口。
        let role: publicentity::RoleCodeOf = entitybase::generate_dynamic_role_code(publicentity::MODULE_TAG, &cid, 0, 18).try_into().unwrap();
        let allowed = || RuntimeCitizenIdentityAuthority::can_manage_voting_identity(
            &who, &cid, &role, b"NX", b"013", identity::CitizenIdentityLevel::Voting, ACTION_REGISTER_VOTING_IDENTITY);
        assert!(!allowed());
        let admins: publicadmin::AdminsOf<Runtime> = vec![adminbase::Admin {
            account_id: who.clone(), cid_number: Default::default(),
            family_name: "测".as_bytes().to_vec().try_into().unwrap(),
            given_name: "试".as_bytes().to_vec().try_into().unwrap(),
        }].try_into().unwrap();
        publicadmin::AdminAccounts::<Runtime>::insert(&cid, adminbase::InstitutionAdmins { institution_code: *b"CREG", admins });
        complete_test_public_wallets(&cid);
        assert!(!allowed(), "只有管理员名册不授予业务权限");
        publicentity::InstitutionRoles::<Runtime>::insert(&cid, &role, entitybase::InstitutionRole {
            cid_number: cid.clone(), role_code: role.clone(), role_name: "测试动态岗位".as_bytes().to_vec().try_into().unwrap(),
            term_required: true, role_status: entitybase::InstitutionRoleStatus::Active,
        });
        let assignments: publicentity::institution::role::RoleAssignmentsOf<Runtime> = vec![entitybase::InstitutionAdminAssignment {
            cid_number: cid.clone(), account_id: who.clone(), role_code: role.clone(),
            term_start: 1, term_end: u32::MAX, assignment_source: entitybase::InstitutionAssignmentSource::InstitutionGovernance,
            assignment_source_ref: vec![18].try_into().unwrap(), assignment_status: entitybase::InstitutionAssignmentStatus::Active,
        }].try_into().unwrap();
        publicentity::InstitutionRoleAssignments::<Runtime>::insert(&cid, &role, &assignments);
        assert!(!allowed(), "有效任职仍需具体业务权限");
        let permissions: publicentity::RolePermissionsOf<Runtime> = [
            (MODULE_CITIZEN_IDENTITY, ACTION_REGISTER_VOTING_IDENTITY),
            (MODULE_CITIZEN_IDENTITY, ACTION_OCCUPY_CID),
            (MODULE_CITIZEN_IDENTITY, ACTION_ADMIN_REBIND_CID_ACCOUNT_ID),
            (MODULE_CITIZEN_IDENTITY, ACTION_REVOKE_CID),
            (MODULE_ADDRESS_REGISTRY, ACTION_SET_ADDRESS),
            (MODULE_INSTITUTION_REGISTRATION, ACTION_REGISTER_INSTITUTION),
        ].into_iter().map(|(module, action)| RoleBusinessPermission {
            role_subject: RoleSubject { cid_number: cid.clone(), role_code: role.clone() },
            business_action_id: entitybase::BusinessActionId { module_tag: module.to_vec().try_into().unwrap(), action_code: action },
            operation: RolePermissionOperation::Propose,
        }).collect::<Vec<_>>().try_into().unwrap();
        publicentity::InstitutionRolePermissions::<Runtime>::insert(&cid, &role, permissions);
        assert!(allowed());
        for (province, city) in [(b"GZ".as_slice(), b"013".as_slice()), (b"NX".as_slice(), b"014".as_slice())] {
            assert!(!RuntimeCitizenIdentityAuthority::can_manage_voting_identity(&who, &cid, &role, province, city, identity::CitizenIdentityLevel::Voting, ACTION_REGISTER_VOTING_IDENTITY));
            assert!(!RuntimeAddressAuthority::can_update_address(&who, &cid, &role, province, city, ACTION_SET_ADDRESS));
        }
        assert!(!RuntimeCitizenIdentityAuthority::can_manage_voting_identity(&who, &cid, &role, b"NX", b"013", identity::CitizenIdentityLevel::Candidate, ACTION_UPGRADE_CANDIDATE_IDENTITY));
        for action in [ACTION_OCCUPY_CID, ACTION_ADMIN_REBIND_CID_ACCOUNT_ID, ACTION_REVOKE_CID] {
            assert!(RuntimeCitizenIdentityAuthority::can_manage_anonymous_cid(&who, &cid, &role, action));
        }
        assert!(RuntimeAddressAuthority::can_update_address(&who, &cid, &role, b"NX", b"013", ACTION_SET_ADDRESS));
        assert!(!RuntimeAddressAuthority::can_update_catalog(&who, &cid, &role, ACTION_SET_ADDRESS_CATALOG));
        assert!(!<RuntimeRegistryAuthority as entitybase::RegistryAuthority<AccountId>>::can_register_institution_origin(&who, &cid, &role, &cid, *b"CREG"));
        publicentity::InstitutionRoleAssignments::<Runtime>::mutate(&cid, &role, |rows| rows[0].term_end = 1);
        assert!(!allowed(), "过期任职不得继续办理");
        publicentity::InstitutionRoleAssignments::<Runtime>::insert(&cid, &role, &assignments);
        publicentity::InstitutionRoleAssignments::<Runtime>::mutate(&cid, &role, |rows| rows[0].assignment_status = entitybase::InstitutionAssignmentStatus::Ended);
        assert!(!allowed(), "已结束任职不得继续办理");
        publicentity::InstitutionRoleAssignments::<Runtime>::insert(&cid, &role, &assignments);
        publicentity::InstitutionRoles::<Runtime>::mutate(&cid, &role, |stored| stored.as_mut().unwrap().role_status = entitybase::InstitutionRoleStatus::Inactive);
        assert!(!allowed(), "停用岗位不得继续办理");
        publicentity::InstitutionRoles::<Runtime>::mutate(&cid, &role, |stored| stored.as_mut().unwrap().role_status = entitybase::InstitutionRoleStatus::Active);
        publicadmin::AdminAccounts::<Runtime>::remove(&cid);
        assert!(!allowed(), "从名册移除后原任职不得继续授权");
    });
}


#[test]
fn unified_registration_runtime_initializes_creg_and_preserves_frg_scope_and_fee_payer() {
    use codec::Encode;
    use entitybase::InstitutionMultisigQuery;
    use entitybase::{
        business_action::*, InstitutionGovernanceAction, InstitutionRoleMutation,
        RolePermissionOperation,
    };
    use identity::CitizenIdentityAuthority;
    use onchain::CallFeeRoute;
    new_test_ext().execute_with(|| {
        let target = test_cid_number(b"NX013-CREGN-937676656-2026");
        let frg = primitives::governance_skeleton::federal_registry_institution();
        let actor = test_cid_number(frg.cid_number.as_bytes());
        let role: publicentity::RoleCodeOf =
            primitives::governance_skeleton::province_commissioner_role_code(*b"NX")
                .try_into()
                .unwrap();
        let signer = publicentity::InstitutionRoleAssignments::<Runtime>::get(&actor, &role)[0]
            .account_id
            .clone();
        let people = [AccountId::new([91; 32]), AccountId::new([92; 32])];
        let seeded:Vec<adminbase::Admin<AccountId>>=people
                .iter()
                .map(|account| adminbase::Admin {
                    account_id: account.clone(),
                    cid_number: Default::default(),
                    family_name: "测".as_bytes().to_vec().try_into().unwrap(),
                    given_name: "试".as_bytes().to_vec().try_into().unwrap(),
                })
                .collect();
        let action = InstitutionGovernanceAction::MutateRolesAndAssignments {
            role_mutations: vec![InstitutionRoleMutation::Create {
                role_name: "测试动态岗位".as_bytes().to_vec(),
                term_required: false,
                permissions: [
                    (
                        publicentity::MODULE_TAG,
                        ACTION_INSTITUTION_GOVERNANCE,
                        RolePermissionOperation::Propose,
                    ),
                    (
                        publicentity::MODULE_TAG,
                        ACTION_INSTITUTION_GOVERNANCE,
                        RolePermissionOperation::Vote,
                    ),
                    (
                        MODULE_CITIZEN_IDENTITY,
                        ACTION_REGISTER_VOTING_IDENTITY,
                        RolePermissionOperation::Propose,
                    ),
                ]
                .into_iter()
                .map(
                    |(module, action_code, operation)| entitybase::RolePermissionSpec {
                        business_action_id: entitybase::BusinessActionId {
                            module_tag: module.to_vec(),
                            action_code,
                        },
                        operation,
                    },
                )
                .collect(),
                assignments: people
                    .iter()
                    .map(|account| entitybase::InstitutionAssignmentTarget {
                        account_id: account.clone(),
                        term_start: 0,
                        term_end: 0,
                        assignment_source: entitybase::InstitutionAssignmentSource::Registry,
                        assignment_source_ref: Vec::new(),
                        assignment_status: entitybase::InstitutionAssignmentStatus::Active,
                    })
                    .collect(),
            }],
            assignment_changes: Vec::new(),
            legal_representative_change: None,
        };
        assert_ok!(PublicManage::register_institution_admins(
            RuntimeOrigin::signed(signer.clone()),
            target.clone(),
            InstitutionGovernanceAction::RegisterAdminWalletChanges {
                added: seeded.clone(),
                removed: vec![],
                replaced: vec![]
            },
            actor.to_vec(),
            role.to_vec(),
            PublicManage::registration_state_hash(&target).unwrap(),
            None
        ));
        let expected = PublicManage::registration_state_hash(&target).unwrap();
        let call = RuntimeCall::PublicManage(publicentity::Call::register_institution_admins {
            cid_number: target.clone(),
            action: action.clone(),
            actor_cid_number: actor.to_vec(),
            actor_role_code: role.to_vec(),
            expected_state_hash: expected,
            initial_threshold: Some(2),
        });
        assert_eq!(
            call.encode(),
            (
                30u8,
                9u8,
                target.clone(),
                action.clone(),
                actor.to_vec(),
                role.to_vec(),
                expected,
                Some(2u32)
            )
                .encode()
        );
        let private_target = test_cid_number(
            primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION
                .cid_number
                .as_bytes(),
        );
        let private_call =
            RuntimeCall::PrivateManage(privateentity::Call::register_institution_admins {
                cid_number: private_target.clone(),
                action: action.clone(),
                actor_cid_number: actor.to_vec(),
                actor_role_code: role.to_vec(),
                expected_state_hash: expected,
                initial_threshold: Some(2),
            });
        assert_eq!(
            private_call.encode(),
            (
                31u8,
                9u8,
                private_target,
                action.clone(),
                actor.to_vec(),
                role.to_vec(),
                expected,
                Some(2u32)
            )
                .encode()
        );
        let normal_limit = RuntimeBlockWeights::get()
            .get(frame_support::dispatch::DispatchClass::Normal)
            .max_extrinsic
            .unwrap();
        assert!(
            PublicManage::registration_weight(&target, &action).all_lte(normal_limit),
            "普通首次登记必须可进入正常区块预算"
        );
        let fee_account = RuntimeInstitutionQuery::lookup_institution_account(
            &actor,
            primitives::account_derive::RESERVED_NAME_FEE,
        )
        .unwrap();
        assert_eq!(
            RuntimeFeeRouter::fee_route(&signer, &call),
            primitives::fee_policy::FeeRoute::Onchain {
                transaction_amount: 0,
                payer_account_id: fee_account
            }
        );
        let other_role: publicentity::RoleCodeOf =
            primitives::governance_skeleton::province_commissioner_role_code(*b"GZ")
                .try_into()
                .unwrap();
        let other_signer =
            publicentity::InstitutionRoleAssignments::<Runtime>::get(&actor, &other_role)[0]
                .account_id
                .clone();
        assert_noop!(
            PublicManage::register_institution_admins(
                RuntimeOrigin::signed(other_signer),
                target.clone(),
                action.clone(),
                actor.to_vec(),
                other_role.to_vec(),
                expected,
                Some(2)
            ),
            publicentity::Error::<Runtime>::RegistryAuthorityDenied
        );
        assert_ok!(PublicManage::register_institution_admins(
            RuntimeOrigin::signed(signer.clone()),
            target.clone(),
            action.clone(),
            actor.to_vec(),
            role.to_vec(),
            expected,
            Some(2)
        ));
        let created = publicentity::InstitutionRoles::<Runtime>::iter_prefix(&target)
            .find(|(_, item)| item.role_name.as_slice() == "测试动态岗位".as_bytes())
            .unwrap()
            .0;
        assert!(RuntimeCitizenIdentityAuthority::can_manage_voting_identity(
            &people[0],
            &target,
            &created,
            b"NX",
            b"013",
            identity::CitizenIdentityLevel::Voting,
            ACTION_REGISTER_VOTING_IDENTITY
        ));
        assert!(
            !RuntimeCitizenIdentityAuthority::can_manage_voting_identity(
                &people[0],
                &target,
                &created,
                b"GZ",
                b"013",
                identity::CitizenIdentityLevel::Voting,
                ACTION_REGISTER_VOTING_IDENTITY
            )
        );
        // 上级注册局不能借通用任职登记改任受保护基金会创世岗位。
        let foundation = test_cid_number(
            primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION
                .cid_number
                .as_bytes(),
        );
        let mut foundation_admins = privateadmin::AdminAccounts::<Runtime>::get(&foundation)
            .unwrap()
            .admins
            .to_vec();
        let outsider = AccountId::new([93; 32]);
        foundation_admins.push(adminbase::Admin {
            account_id: outsider.clone(),
            cid_number: Default::default(),
            family_name: b"F".to_vec().try_into().unwrap(),
            given_name: b"G".to_vec().try_into().unwrap(),
        });
        let protected = InstitutionGovernanceAction::ReplaceAdminsAndMutateRoles {
            admins: foundation_admins,
            role_mutations: Vec::new(),
            assignment_changes: vec![entitybase::InstitutionRoleAssignmentChange {
                role_code: primitives::cid::china::citizenchain::ROLE_CODE_GENESIS_PRODUCT_MANAGER
                    .to_vec(),
                assignments: vec![entitybase::InstitutionAssignmentTarget {
                    account_id: outsider,
                    term_start: 0,
                    term_end: 0,
                    assignment_source: entitybase::InstitutionAssignmentSource::Registry,
                    assignment_source_ref: Vec::new(),
                    assignment_status: entitybase::InstitutionAssignmentStatus::Active,
                }],
            }],
            legal_representative_change: None,
        };
        let gz_role: publicentity::RoleCodeOf =
            primitives::governance_skeleton::province_commissioner_role_code(*b"GZ")
                .try_into()
                .unwrap();
        let gz_actor = publicentity::InstitutionRoleAssignments::<Runtime>::get(&actor, &gz_role)
            [0]
        .account_id
        .clone();
        let expected_foundation = PrivateManage::registration_state_hash(&foundation).unwrap();
        assert_noop!(
            PrivateManage::register_institution_admins(
                RuntimeOrigin::signed(gz_actor),
                foundation,
                protected,
                actor.to_vec(),
                gz_role.to_vec(),
                expected_foundation,
                None
            ),
            privateentity::Error::<Runtime>::InvalidRegistrationAction
        );
        assert_noop!(
            PublicManage::register_institution_admins(
                RuntimeOrigin::signed(signer),
                target,
                action,
                actor.to_vec(),
                role.to_vec(),
                expected,
                Some(2)
            ),
            publicentity::Error::<Runtime>::StaleRegistrationState
        );
    });
}


#[test]
fn evm_account_creation_weights_cover_fee_rewards_and_background_payouts() {
    use frame_support::{traits::Get, weights::Weight};
    let mapping = crate::evm_accounts::EvmAccountMapping::get();
    macro_rules! budget {
        ($config:path, $interface:path, $base:ty, $method:ident, $count:expr) => {
            assert_eq!(
                <<Runtime as $config>::WeightInfo as $interface>::$method(),
                <$base as $interface>::$method()
                    .saturating_add(mapping.saturating_mul($count as u64)),
                stringify!($method),
            );
        };
    }
    budget!(
        pallet_transaction_payment::Config,
        pallet_transaction_payment::WeightInfo,
        pallet_transaction_payment::weights::SubstrateWeight<Runtime>,
        charge_transaction_payment,
        3
    );
    budget!(
        onchain::pallet::Config,
        onchain::weights::WeightInfo,
        onchain::weights::SubstrateWeight<Runtime>,
        transfer_with_remark,
        1
    );
    let ordinary = <<Runtime as onchain::pallet::Config>::WeightInfo as onchain::weights::WeightInfo>::transfer_with_remark();
    let payment = <<Runtime as pallet_transaction_payment::Config>::WeightInfo as pallet_transaction_payment::WeightInfo>::charge_transaction_payment();
    let old = <onchain::weights::SubstrateWeight<Runtime> as onchain::weights::WeightInfo>::transfer_with_remark()
        .saturating_add(<pallet_transaction_payment::weights::SubstrateWeight<Runtime> as pallet_transaction_payment::WeightInfo>::charge_transaction_payment());
    assert_eq!(
        ordinary.saturating_add(payment),
        old.saturating_add(mapping.saturating_mul(4)),
        "ordinary principal and common fee receivers must not be counted twice"
    );

    budget!(
        fullnodeissue::Config,
        fullnodeissue::weights::WeightInfo,
        fullnodeissue::weights::SubstrateWeight<Runtime>,
        on_finalize_reward,
        1
    );
    budget!(
        fullnodeissue::Config,
        fullnodeissue::weights::WeightInfo,
        fullnodeissue::weights::SubstrateWeight<Runtime>,
        bind_reward_account,
        0
    );
    budget!(
        fullnodeissue::Config,
        fullnodeissue::weights::WeightInfo,
        fullnodeissue::weights::SubstrateWeight<Runtime>,
        rebind_reward_account,
        0
    );
    budget!(
        citizenissue::Config,
        citizenissue::weights::WeightInfo,
        citizenissue::weights::SubstrateWeight<Runtime>,
        on_voting_identity_registered,
        1
    );
    budget!(
        prbbankissue::Config,
        prbbankissue::weights::WeightInfo,
        prbbankissue::weights::SubstrateWeight<Runtime>,
        on_finalize_settlement,
        primitives::cid::china::china_ch::CHINA_CH.len()
    );
    budget!(
        personalentity::Config,
        personalentity::weights::WeightInfo,
        personalentity::weights::SubstrateWeight<Runtime>,
        propose_close,
        4
    );
    assert_eq!(
        <<Runtime as personalentity::Config>::WeightInfo as personalentity::weights::WeightInfo>::propose_create(3),
        <personalentity::weights::SubstrateWeight<Runtime> as personalentity::weights::WeightInfo>::propose_create(3)
            .saturating_add(mapping.saturating_mul(4)),
    );
    for (method, actual, original) in [
        ("multisig transfer", <RuntimeMultisigWeights as multisig::weights::WeightInfo>::propose_transfer(),
            <multisig::weights::SubstrateWeight<Runtime> as multisig::weights::WeightInfo>::propose_transfer()
                .saturating_add(Weight::from_parts(10_000_000, 3_000))
                .saturating_add(<Runtime as frame_system::Config>::DbWeight::get().reads(1))),
        ("safety fund", <RuntimeMultisigWeights as multisig::weights::WeightInfo>::propose_safety_fund_transfer(),
            <multisig::weights::SubstrateWeight<Runtime> as multisig::weights::WeightInfo>::propose_safety_fund_transfer()),
        ("fee sweep", <RuntimeMultisigWeights as multisig::weights::WeightInfo>::propose_sweep_to_main(),
            <multisig::weights::SubstrateWeight<Runtime> as multisig::weights::WeightInfo>::propose_sweep_to_main()),
    ] {
        assert_eq!(actual, original.saturating_add(mapping.saturating_mul(4)), "{method}");
    }
    let original_joint = <upgrade::weights::SubstrateWeight<Runtime> as upgrade::weights::WeightInfo>::propose_runtime_upgrade()
        .max(<resolutionissue::weights::SubstrateWeight<Runtime> as resolutionissue::weights::WeightInfo>::propose_issuance());
    let allocation_budget = mapping.saturating_mul(ResolutionIssuanceMaxAllocations::get() as u64);
    assert_eq!(
        <RuntimeJointVoteResultCallback as engingcore::JointVoteResultCallback>::execution_weight(),
        Weight::from_parts(original_joint.ref_time(), 0).saturating_add(allocation_budget),
        "mapping proof budget must survive the existing callback proof reset",
    );
    budget!(
        square::Config,
        square::weights::WeightInfo,
        square::weights::SubstrateWeight<Runtime>,
        subscribe,
        1
    );
    budget!(
        square::Config,
        square::weights::WeightInfo,
        square::weights::SubstrateWeight<Runtime>,
        change_subscription_plan,
        1
    );
    budget!(
        square::Config,
        square::weights::WeightInfo,
        square::weights::SubstrateWeight<Runtime>,
        process_one_due,
        1
    );
    budget!(
        square::Config,
        square::weights::WeightInfo,
        square::weights::SubstrateWeight<Runtime>,
        renewal_success,
        1
    );
    budget!(
        square::Config,
        square::weights::WeightInfo,
        square::weights::SubstrateWeight<Runtime>,
        renewal_creator,
        1
    );
    budget!(
        square::Config,
        square::weights::WeightInfo,
        square::weights::SubstrateWeight<Runtime>,
        on_initialize_base,
        0
    );
    budget!(
        square::Config,
        square::weights::WeightInfo,
        square::weights::SubstrateWeight<Runtime>,
        publish_post,
        0
    );
}

/// 最终确认区块 107 的旧提案 0 原始键值；直接内嵌，禁止生成额外夹具文件。
#[test]
fn proposal_id_upgrade_preserves_existing_proposal_and_migrates_constitution_encoding() {
    use codec::Encode;
    use frame_support::traits::{OnRuntimeUpgrade, StorageVersion};
    new_test_ext().execute_with(|| {
        let old_rows: Vec<(String, String)> = serde_json::from_str(r#"[["0x940b0e4fbd2321a3969f62ab78f429108acca8665c54f33eb487a1d7f3b59c25","0xea07"],["0x940b0e4fbd2321a3969f62ab78f42910ef71665d8dae6a883da700b7f4e11259","0x01000000"],["0x940b0e4fbd2321a3969f62ab78f429102f58f30b61a6ff2e8c744b01d7249e0c","0x0100000000000000"],["0x940b0e4fbd2321a3969f62ab78f4291088c2f7188c6fdd1dffae2fa0d171f440c804ce198ec337e3dc762bdd1a09aece0000000000000000","0x000003014e52430001684c4e3030312d4e524330472d3934343830353136352d32303236017c0c099ee4df10c5bd3f618ddf132b6d15390fa27d2c1369f70aeb6b5f3907e504684c4e3030312d4e524330472d3934343830353136352d3230323658000000781c0000"],["0x940b0e4fbd2321a3969f62ab78f42910ac1871b31bf1a157b8c41a711a1b7f5d05dcc4600dd6efb06e6cddee227c8945791c0000","0x040000000000000000"],["0x940b0e4fbd2321a3969f62ab78f42910113cc4a45cdac965b8536c3bb68b3105c804ce198ec337e3dc762bdd1a09aece0000000000000000","0x206d756c746973696700000000206d756c746973696700684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d4245520400684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d42455200f90f939f2c16af0b247be325c97200de7d28cb0912d607c305942861a4c2ef68"],["0x940b0e4fbd2321a3969f62ab78f42910be6e08c284900d6188163af99c8f32abc804ce198ec337e3dc762bdd1a09aece0000000000000000272eb39721706017a72568b4dbfa1bf500684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d424552","0x4c2837cdcbf8876af4cbea8bd2a2025cf978a10cd82d7c2abf1dc75ed753046702089083a3cc675ecd00992888ea5f07a1a33a75aae6028b8b9647e322cca4446d92f660e7847590e0ec2477292c08a2a636eb434e8e90d046f50aee9e46950c02b8178d5bd83863384a434978d60690a0d1878b378fef0e871d1c055e22a90677240a846236c82599cb3579b4a3f603bfa154c9e1fcc2148de7f87976fdcad5031853095cb03fa48f31d79664de8a6b6d5dcbb0a1df4c849d984aea94cb38b838c2ac72d5a5ded1bd353e3f8b9e12e4cc26b3a964abc86f3d36fedf7aa7801041e4af7f1dd82567cbef1c27e10fc17fc4c51db98856a3c13043ff85bfb2c96171b214e383f128cb490205848fa14d81461269b4a3f6e4ca9a1c7c931f5f1ec9675040647c9dc3b24a91068781d2bac726df955480d94a0c3e535b0f3cc661297beef25f0dc68a1e1e250d61ea431c3e9b17ad1a1da79a05dffe4649e086fa9c3800339746be531077a625d4cae0c9ed8a6d96822f1c1a1ab1d5230b1f3614a8420ce3e7c321eae667130725a2ed824d793944dcf940755a65f6e1ce88a4c1a95134b8ac7127c63ecc5023d68a5cee90383624a588a376def423ee0af888ba96642edb16bf528f3ff1495f1a78be229efe506538db8f122f1a676f59209711b7636c5cd20ac500c37a7204d8137101259edfa84499ce1b05b5e9a6a6adc17d040336450c6c0a2bf7e8efade878819d9d64a8358123ec2dc6af00420e96a3734706ac1e28c5926304e9277e015b48b947c7f4fac939374f1e906849bfd89cd43d41749f82ecf2d2efda27a1965c0de91224bd3c1ceda2dfbe349027b1e368a51a72"],["0x940b0e4fbd2321a3969f62ab78f42910d1eed86c0f3b72780c5e625e0afb9e1ec804ce198ec337e3dc762bdd1a09aece000000000000000034d0a423b5e137e95e7bc919c6e5d546684c4e3030312d4e524330472d3934343830353136352d32303236","0x13000000"],["0x940b0e4fbd2321a3969f62ab78f42910e54f94641d7d44c0c1ea4675a3af0d57c804ce198ec337e3dc762bdd1a09aece0000000000000000","0xb9026d756c746973696701684c4e3030312d4e524330472d3934343830353136352d323032367c0c099ee4df10c5bd3f618ddf132b6d15390fa27d2c1369f70aeb6b5f3907e5bdd95642f8ea916a02859dcb8d6cd563474bc03ceeeeeeeeeeeeeeeeeeeeeeeea086010000000000000000000000000064e6b58be696b07761736de79a84e4bba5e5a4aae585bce5aeb92837cdcbf8876af4cbea8bd2a2025cf978a10cd82d7c2abf1dc75ed753046702"],["0x940b0e4fbd2321a3969f62ab78f42910cb8f88266dcb33b7dc4054c2c56a9b12c804ce198ec337e3dc762bdd1a09aece0000000000000000","0x206d756c7469736967"],["0x940b0e4fbd2321a3969f62ab78f4291081b9aa7d67184f022ad57b3022119a34c804ce198ec337e3dc762bdd1a09aece0000000000000000","0x580000000166000000"],["0x940b0e4fbd2321a3969f62ab78f429101d0e4850dc45d8460af68508684ecb98bb1bdbcacd6ac9340000000000000000","0xc75400000000000000000000"],["0x940b0e4fbd2321a3969f62ab78f429106e5b6ee014398d36aaf08160d086afbd0f9a1987a75d08a737fd09a13db7e35200684c4e3030312d4e524330472d3934343830353136352d32303236","0x00"],["0x940b0e4fbd2321a3969f62ab78f42910c3b7e8ee9a87e612aff0da100757ddc0c804ce198ec337e3dc762bdd1a09aece0000000000000000","0xea0700000000"],["0x940b0e4fbd2321a3969f62ab78f429108c68a587d1a776a2be607a371f00993a0ceac3169ca8a02a4e524300bb1bdbcacd6ac9340000000000000000","0x"],["0x940b0e4fbd2321a3969f62ab78f42910b7a345e83b2ef8ae370992f95123a1a2fcc2ecb4f16cb890684c4e3030312d4e524330472d3934343830353136352d32303236bb1bdbcacd6ac9340000000000000000","0x"],["0x940b0e4fbd2321a3969f62ab78f4291022bcf5038ca8f3a8212dbf8e0e2ec224aa2fef6e3e773278206d756c7469736967bb1bdbcacd6ac9340000000000000000","0x"],["0x940b0e4fbd2321a3969f62ab78f42910885776b620ebedb3f2ef1522c9c3af21a204d8aae77d8e94ea07bb1bdbcacd6ac9340000000000000000","0x"],["0xc2c566f58c1a6eb3a58088f5662cf9cde3c3fe6496d9716810203b024b48195ec804ce198ec337e3dc762bdd1a09aece0000000000000000238312960aab452b33cd5e380194c1aa01684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d4245526c5cd20ac500c37a7204d8137101259edfa84499ce1b05b5e9a6a6adc17d0403","0x01"],["0xc2c566f58c1a6eb3a58088f5662cf9cde3c3fe6496d9716810203b024b48195ec804ce198ec337e3dc762bdd1a09aece00000000000000004467409fd24db031db310e3f8e7c89cb01684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d4245521853095cb03fa48f31d79664de8a6b6d5dcbb0a1df4c849d984aea94cb38b838","0x01"],["0xc2c566f58c1a6eb3a58088f5662cf9cde3c3fe6496d9716810203b024b48195ec804ce198ec337e3dc762bdd1a09aece000000000000000048c098b9038a38d63a5318030b95449d01684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d424552240a846236c82599cb3579b4a3f603bfa154c9e1fcc2148de7f87976fdcad503","0x01"],["0xc2c566f58c1a6eb3a58088f5662cf9cde3c3fe6496d9716810203b024b48195ec804ce198ec337e3dc762bdd1a09aece000000000000000049bbb60cf6d57d556707fccd3668404901684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d4245525040647c9dc3b24a91068781d2bac726df955480d94a0c3e535b0f3cc661297b","0x01"],["0xc2c566f58c1a6eb3a58088f5662cf9cde3c3fe6496d9716810203b024b48195ec804ce198ec337e3dc762bdd1a09aece00000000000000005f41013016e5db192dff1b1d476474de01684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d424552e4af7f1dd82567cbef1c27e10fc17fc4c51db98856a3c13043ff85bfb2c96171","0x00"],["0xc2c566f58c1a6eb3a58088f5662cf9cde3c3fe6496d9716810203b024b48195ec804ce198ec337e3dc762bdd1a09aece00000000000000008714e5f670127fafe96abb10f138211f01684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d424552eef25f0dc68a1e1e250d61ea431c3e9b17ad1a1da79a05dffe4649e086fa9c38","0x01"],["0xc2c566f58c1a6eb3a58088f5662cf9cde3c3fe6496d9716810203b024b48195ec804ce198ec337e3dc762bdd1a09aece000000000000000089a971a82676e62395892a2ce29580f401684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d4245522edb16bf528f3ff1495f1a78be229efe506538db8f122f1a676f59209711b763","0x01"],["0xc2c566f58c1a6eb3a58088f5662cf9cde3c3fe6496d9716810203b024b48195ec804ce198ec337e3dc762bdd1a09aece0000000000000000924d852def22ce3da3002f6cb32fc97e01684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d4245520ce3e7c321eae667130725a2ed824d793944dcf940755a65f6e1ce88a4c1a951","0x01"],["0xc2c566f58c1a6eb3a58088f5662cf9cde3c3fe6496d9716810203b024b48195ec804ce198ec337e3dc762bdd1a09aece0000000000000000a6903b89ba63b7042c2bb96bd887b85501684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d424552089083a3cc675ecd00992888ea5f07a1a33a75aae6028b8b9647e322cca4446d","0x01"],["0xc2c566f58c1a6eb3a58088f5662cf9cde3c3fe6496d9716810203b024b48195ec804ce198ec337e3dc762bdd1a09aece0000000000000000b7a7f6cfb70923f50dac63f298cc526f01684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d42455200339746be531077a625d4cae0c9ed8a6d96822f1c1a1ab1d5230b1f3614a842","0x01"],["0xc2c566f58c1a6eb3a58088f5662cf9cde3c3fe6496d9716810203b024b48195ec804ce198ec337e3dc762bdd1a09aece0000000000000000b9d75cc88465724491fd34e170ccfe4001684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d424552b8178d5bd83863384a434978d60690a0d1878b378fef0e871d1c055e22a90677","0x01"],["0xc2c566f58c1a6eb3a58088f5662cf9cde3c3fe6496d9716810203b024b48195ec804ce198ec337e3dc762bdd1a09aece0000000000000000cbb10ef529157b52087708e36f53678201684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d42455234b8ac7127c63ecc5023d68a5cee90383624a588a376def423ee0af888ba9664","0x01"],["0xc2c566f58c1a6eb3a58088f5662cf9cde3c3fe6496d9716810203b024b48195ec804ce198ec337e3dc762bdd1a09aece0000000000000000d6f0ab9f69e4ba1353b3d58964d95c1401684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d42455292f660e7847590e0ec2477292c08a2a636eb434e8e90d046f50aee9e46950c02","0x00"],["0xc2c566f58c1a6eb3a58088f5662cf9cde3c3fe6496d9716810203b024b48195ec804ce198ec337e3dc762bdd1a09aece0000000000000000e3bf27f157da89a410e1b931356768a301684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d424552b214e383f128cb490205848fa14d81461269b4a3f6e4ca9a1c7c931f5f1ec967","0x01"],["0xc2c566f58c1a6eb3a58088f5662cf9cde3c3fe6496d9716810203b024b48195ec804ce198ec337e3dc762bdd1a09aece0000000000000000f43fce72c9723a3f251ada340cce4bef01684c4e3030312d4e524330472d3934343830353136352d3230323640434f4d4d49545445455f4d454d4245522837cdcbf8876af4cbea8bd2a2025cf978a10cd82d7c2abf1dc75ed753046702","0x01"],["0xc2c566f58c1a6eb3a58088f5662cf9cda352da679dea4257d0e569b8f20f350ec804ce198ec337e3dc762bdd1a09aece0000000000000000","0x0d00000002000000"],["0xc2c566f58c1a6eb3a58088f5662cf9cd199451d5eede742a4124c82f7e6f1838c804ce198ec337e3dc762bdd1a09aece0000000000000000","0x0d000000"],["0xc2c566f58c1a6eb3a58088f5662cf9cdc4852d3215287c04cb1af4612942a2a0c804ce198ec337e3dc762bdd1a09aece0000000000000000","0x00"]]"#).unwrap();
        for (key, value) in &old_rows { sp_io::storage::set(&hex::decode(&key[2..]).unwrap(), &hex::decode(&value[2..]).unwrap()); }
        let law_key = legislation::LawVersions::<Runtime>::hashed_key_for(0u64, 1u32);
        let law_before = legislation::LawVersions::<Runtime>::get(0, 1).unwrap();
        let law_new = sp_io::storage::get(&law_key).unwrap();
        // 创世占位 ID 仍为 0；仅还原旧版尾部编码，正文全部采用真实创世配置。
        let offset = law_new.len() - 32;
        let mut law_old = law_new[..offset].to_vec(); law_old.extend(0u64.encode()); law_old.extend_from_slice(&law_new[offset + 16..]);
        assert_eq!(law_old.len(), 226_673);
        assert_eq!(hex::encode(sp_io::hashing::sha2_256(&law_old)), "185dc5e62bd6832ae45c5349717b23336de24184d78a33034ad28115c6ad9602");
        sp_io::storage::set(&law_key, &law_old);
        let accounts_before = frame_system::Account::<Runtime>::iter().collect::<Vec<_>>();
        let issuance_before = Balances::total_issuance();
        StorageVersion::new(0).put::<engingcore::Pallet<Runtime>>();
        AddPrcFeeSweepPermissions::on_runtime_upgrade();
        assert_eq!(StorageVersion::get::<engingcore::Pallet<Runtime>>(), StorageVersion::new(1));
        assert_eq!(engingcore::NextProposalId::<Runtime>::get(), 1u128);
        assert_eq!(engingcore::CurrentProposalYear::<Runtime>::get(), 2026u32);
        assert_eq!(engingcore::YearProposalCounter::<Runtime>::get(), 1u64);
        let display = engingcore::ProposalDisplayId::<Runtime>::get(0u128).unwrap();
        assert_eq!((display.year, display.seq_in_year), (2026u32, 0u64));
        assert_eq!(engingcore::Proposals::<Runtime>::get(0u128).unwrap().status, engingcore::STATUS_EXECUTED);
        assert_eq!(internalvote::InternalVotesByTicket::<Runtime>::iter_prefix(0u128).count(), 15);
        let tally = internalvote::InternalTallies::<Runtime>::get(0u128);
        assert_eq!((tally.yes, tally.no), (13, 2));
        assert_eq!(internalvote::InternalThresholdSnapshot::<Runtime>::get(0u128), Some(13));
        assert_eq!(engingcore::ProposalsByExpiry::<Runtime>::get(7289).as_slice(), &[0u128]);
        let cleanup = engingcore::ScheduledCleanups::<Runtime>::get(0u64).unwrap();
        assert_eq!((cleanup.cleanup_at, cleanup.proposal_id), (21703, 0u128));
        assert!(engingcore::ProposalsByYear::<Runtime>::contains_key(2026u32, 0u128));
        assert!(engingcore::ProposalsByCode::<Runtime>::contains_key(*b"NRC\0", 0u128));
        let cid: engingcore::types::CidNumber = b"LN001-NRC0G-944805165-2026".to_vec().try_into().unwrap();
        assert!(engingcore::ProposalsByCid::<Runtime>::contains_key(&cid, 0u128));
        let owner: frame_support::BoundedVec<u8, <Runtime as engingcore::Config>::MaxModuleTagLen> = b"multisig".to_vec().try_into().unwrap();
        assert!(engingcore::ProposalsByOwner::<Runtime>::contains_key(&owner, 0u128));
        let voters = engingcore::VoterSnapshot::<Runtime>::iter_prefix(0u128).collect::<Vec<_>>();
        assert_eq!(voters.len(), 1);
        assert_eq!(voters[0].1.len(), 19);
        assert_eq!(engingcore::InstitutionTicketCountSnapshot::<Runtime>::iter_prefix(0u128).next().unwrap().1, 19);

        let migrated_law = sp_io::storage::get(&law_key).unwrap();
        assert_eq!(migrated_law.len(), 226_681);
        assert_eq!(hex::encode(sp_io::hashing::sha2_256(&migrated_law)), "3aa33246151c36fa538e1df3ee5f3fe488f0a39de283600be74cd3e7121529c1");
        assert_eq!(&migrated_law[..offset], &law_old[..offset]);
        assert_eq!(&migrated_law[offset..offset + 16], &0u128.encode());
        assert_eq!(&migrated_law[offset + 16..], &law_old[offset + 8..]);
        assert_eq!(migrated_law, law_new);
        assert_eq!(legislation::LawVersions::<Runtime>::get(0, 1).unwrap().encode(), law_before.encode());
        assert_eq!(frame_system::Account::<Runtime>::iter().collect::<Vec<_>>(), accounts_before);
        assert_eq!(Balances::total_issuance(), issuance_before);
        // 原始主记录、业务载荷、快照及投票值必须逐字节保留，并删除旧 ID 键。
        for name in ["Proposals", "ProposalData", "ProposalVotePlans", "ProposalOwner", "ProposalMeta"] {
            let prefix = frame_support::storage::storage_prefix(b"VotingEngine", name.as_bytes());
            let mut old_key = prefix.to_vec(); old_key.extend(sp_io::hashing::blake2_128(&0u64.encode())); old_key.extend(0u64.encode());
            let mut new_key = prefix.to_vec(); new_key.extend(sp_io::hashing::blake2_128(&0u128.encode())); new_key.extend(0u128.encode());
            let expected = old_rows.iter().find(|(k, _)| *k == format!("0x{}", hex::encode(&old_key))).unwrap();
            assert!(!sp_io::storage::exists(&old_key));
            assert_eq!(sp_io::storage::get(&new_key).unwrap().to_vec(), hex::decode(&expected.1[2..]).unwrap());
        }
        let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
        AddPrcFeeSweepPermissions::on_runtime_upgrade();
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
        assert_eq!(engingcore::Pallet::<Runtime>::allocate_proposal_id().unwrap(), 1u128);
        let display = engingcore::ProposalDisplayId::<Runtime>::get(1u128).unwrap();
        assert_eq!((display.year, display.seq_in_year), (2026u32, 1u64));
    });
}

#[test]
fn proposal_id_upgrade_rejects_invalid_old_layout_without_partial_writes() {
    use frame_support::traits::{OnRuntimeUpgrade, StorageVersion};
    new_test_ext().execute_with(|| {
        StorageVersion::new(0).put::<engingcore::Pallet<Runtime>>();
        sp_io::storage::set(&engingcore::CurrentProposalYear::<Runtime>::hashed_key(), &2026u16.to_le_bytes(),
        );
        sp_io::storage::set(&engingcore::NextProposalId::<Runtime>::hashed_key(), &[1, 2, 3],
        );
        let before = sp_io::storage::root(sp_runtime::StateVersion::V1);
        assert!(std::panic::catch_unwind(|| AddPrcFeeSweepPermissions::on_runtime_upgrade()).is_err());
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), before);
    });
}

#[test]
fn ordinary_transfer_remark_fee_keeps_multisig_creation_fee_unchanged() {
    use frame_support::dispatch::GetDispatchInfo;
    sp_io::TestExternalities::default().execute_with(|| {
        let who = AccountId::new([0x61; 32]);
        for amount in [1, 10_499, 10_500, 100_000, u128::MAX] {
            for remark in [
                vec![],
                b"a".to_vec(),
                "中".as_bytes().to_vec(),
                b" ".to_vec(),
                b"\n".to_vec(),
                vec![b'a'; 99],
            ] {
                let expected = primitives::fee_policy::calculate_onchain_fee(amount)
                    + if remark.is_empty() {
                        0
                    } else {
                        primitives::fee_policy::FLAT_FEE
                    };
                let call =
                    RuntimeCall::OnchainTransaction(onchain::pallet::Call::transfer_with_remark {
                        beneficiary_account_id: AccountId::new([0x62; 32]),
                        amount,
                        remark: remark.clone().try_into().unwrap(),
                    });
                assert_eq!(
                    onchain::charge_details::<Runtime, Balances, RuntimeFeeRouter>(
                        &who,
                        &call,
                        &call.get_dispatch_info(),
                        0,
                    )
                    .unwrap(),
                    Some((who.clone(), expected))
                );
                let multisig =
                    RuntimeCall::MultisigTransfer(multisig::pallet::Call::propose_transfer {
                        actor_cid_number: None,
                        proposer_role_code: None,
                        funding_account_id: who.clone(),
                        beneficiary_account_id: AccountId::new([0x62; 32]),
                        amount,
                        remark: remark.try_into().unwrap(),
                    });
                assert_eq!(
                    onchain::charge_details::<Runtime, Balances, RuntimeFeeRouter>(
                        &who,
                        &multisig,
                        &multisig.get_dispatch_info(),
                        0,
                    )
                    .unwrap(),
                    Some((who.clone(), primitives::fee_policy::ONCHAIN_MIN_FEE))
                );
            }
        }
    });
}

#[test]
fn multisig_execution_charger_uses_existing_payer_and_shared_remark_fee() {
    use primitives::fee_policy::OnchainFeeCharger;
    type Charger = <Runtime as multisig::pallet::Config>::OnchainFeeCharger;
    for (has_remark, expected) in [(false, 50), (true, 150)] {
        sp_io::TestExternalities::default().execute_with(|| {
            System::set_block_number(1);
            let payer = AccountId::new([0x61; 32]);
            assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), payer.clone().into(), 10_000));
            assert_eq!(Charger::charge(&payer, 50_000, has_remark).unwrap(), expected);
            assert_eq!(Balances::free_balance(&payer), 10_000 - expected);
            assert_eq!(System::events().iter().filter(|event| matches!(&event.event,
                RuntimeEvent::OnchainTransaction(onchain::pallet::Event::FeePaid { account_id, fee })
                    if account_id == &payer && *fee == expected)).count(), 1);
        });
    }
}


#[test]
fn institution_wallet_governance_keeps_frg_votes_within_original_province_group() {
    use adminbase::InstitutionAdminWalletQuery;
    use codec::Encode;
    use frame_support::traits::Hooks;
    new_test_ext().execute_with(|| {
        let institution=primitives::governance_skeleton::federal_registry_institution();
        let cid: publicentity::pallet::CidNumberOf<Runtime>=institution.cid_number.as_bytes().to_vec().try_into().unwrap();
        let role: publicentity::RoleCodeOf=primitives::governance_skeleton::province_commissioner_role_code(*b"GD").try_into().unwrap();
        let other_role: publicentity::RoleCodeOf=primitives::governance_skeleton::province_commissioner_role_code(*b"HU").try_into().unwrap();
        let group=publicentity::InstitutionRoleAssignments::<Runtime>::get(&cid,&role).iter().map(|a|a.account_id.clone()).collect::<Vec<_>>();
        let other=publicentity::InstitutionRoleAssignments::<Runtime>::get(&cid,&other_role)[0].account_id.clone();
        assert_eq!(group.len(),5);
        let pair=sr25519::Pair::from_seed(&[93;32]);
        let change=adminbase::InstitutionAdminWalletChange {admin_account:group[1].clone(),expected_wallet:group[1].clone(),new_wallet:AccountId::new(pair.public().0),expected_revision:0,
        };
        let expires_at=1_782_950_400+86400;
        let payload=adminbase::InstitutionWalletAcceptance{genesis_hash:System::block_hash(0),cid_number:cid.to_vec().try_into().unwrap(),change:change.clone(),mode:adminbase::InstitutionWalletChangeMode::Governance,expires_at,
        }.encode();
        let proof=pair.sign(&primitives::sign::signing_message(primitives::sign::OP_SIGN_INSTITUTION_WALLET,&payload,
            )).0.to_vec().try_into().unwrap();
        let action=entitybase::InstitutionGovernanceAction::RotateAdminWallet{change:change.clone(),expires_at,new_wallet_proof:proof,
        };
        assert_noop!(PublicManage::propose_institution_governance(RuntimeOrigin::signed(other.clone()),cid.clone(),action.clone(),cid.to_vec(),other_role.clone()),publicentity::Error::<Runtime>::PermissionDenied);
        assert_ok!(PublicManage::propose_institution_governance(RuntimeOrigin::signed(group[0].clone()),cid.clone(),action,cid.to_vec(),role.clone()));
        let pid=VotingEngine::next_proposal_id()-1;
        let plan=VotingEngine::proposal_vote_plan(pid).unwrap();
        assert_eq!(plan.voter_subjects.len(),1);
        let snapshot=engingcore::VoterSnapshot::<Runtime>::get(pid,&plan.proposer_subject).unwrap();
        assert_eq!(snapshot.len(),5);
        assert!(!snapshot.contains(&other));
        assert!(internalvote::Pallet::<Runtime>::do_internal_vote(other,pid,internalvote::InternalVoteTicketClaim::InstitutionRole(other_role),true).is_err());
        assert_ok!(internalvote::Pallet::<Runtime>::do_internal_vote(group[1].clone(),pid,internalvote::InternalVoteTicketClaim::InstitutionRole(role.clone()),true));
        assert_ok!(internalvote::Pallet::<Runtime>::do_internal_vote(group[2].clone(),pid,internalvote::InternalVoteTicketClaim::InstitutionRole(role.clone()),true));
        <VotingEngine as Hooks<BlockNumber>>::on_initialize(System::block_number());
        assert_eq!(PublicAdmins::current_institution_wallet(adminbase::FRG,&cid,&change.new_wallet),
            Some(change.new_wallet.clone()));
        assert_eq!(publicentity::InstitutionRoleAssignments::<Runtime>::get(&cid,&role).iter().map(|a|a.account_id.clone()).collect::<Vec<_>>(),group);
        assert_eq!(PublicAdmins::institution_admins_len(adminbase::FRG,&cid),Some(215));
    });
}

#[test]
fn institution_wallet_new_pq_proof_is_first_use_scoped_and_operation_safe() {
    use adminbase::{InstitutionAdminQuery, InstitutionAdminWalletQuery};
    use frame_support::{assert_noop, assert_ok};
    new_test_ext().execute_with(|| {
        let institution=primitives::governance_skeleton::federal_registry_institution();
        let cid: publicentity::pallet::CidNumberOf<Runtime>=institution.cid_number.as_bytes().to_vec().try_into().unwrap();
        let accounts_before = account_state_without_wallet_refs().encode();
        let issuance_before = Balances::total_issuance();
        let roles_before = publicentity::InstitutionRoleAssignments::<Runtime>::iter_prefix(&cid)
            .collect::<Vec<_>>()
            .encode();
        let anchor=crate::PublicAdmins::institution_admins(adminbase::FRG,&cid).unwrap()[0].clone();
        let child=[94;32]; let public=mldsa::public_key(&child); let new=AccountId::new(mldsa::account_id(&public));
        let expires_at=1_782_950_400+180;
        let change=adminbase::InstitutionAdminWalletChange{admin_account:anchor.clone(),expected_wallet:anchor.clone(),new_wallet:new.clone(),expected_revision:0,
        };
        let payload=adminbase::InstitutionWalletAcceptance{genesis_hash:System::block_hash(0),cid_number:cid.to_vec().try_into().unwrap(),change:change.clone(),mode:adminbase::InstitutionWalletChangeMode::SelfSigned,expires_at,
        }.encode();
        let signature=mldsa::sign(&child,&primitives::sign::signing_message(primitives::sign::OP_SIGN_INSTITUTION_WALLET,&payload,
            ),
        ).unwrap();
        let proof:adminbase::InstitutionWalletProof=mldsa::encode_pq_signature(&public,&signature).try_into().unwrap();
        <mldsa::GenesisChainPhase<Runtime> as mldsa::ChainPhase>::enter_operation();
        assert!(!mldsa::PqPublicKey::<Runtime>::contains_key(&new));
        let mut forged=proof.clone();forged[2000]^=1;
        assert_noop!(crate::PublicManage::rotate_institution_admin_wallet(crate::RuntimeOrigin::signed(anchor.clone()),cid.clone(),change.clone(),expires_at,forged),publicentity::Error::<Runtime>::InvalidWalletProof);
        assert_ok!(crate::PublicManage::rotate_institution_admin_wallet(crate::RuntimeOrigin::signed(anchor.clone()),cid.clone(),change,expires_at,proof));
        assert_eq!(crate::PublicAdmins::current_institution_wallet(adminbase::FRG,&cid,&new),Some(new.clone()));
        assert_eq!(crate::PublicAdmins::resolve_admin_account(adminbase::FRG,&cid,&new),Some(new.clone())
        );
        assert_eq!(
            account_state_without_wallet_refs().encode(),
            accounts_before
        );
        assert_eq!(Balances::total_issuance(), issuance_before);
        assert_eq!(
            publicentity::InstitutionRoleAssignments::<Runtime>::iter_prefix(&cid)
                .collect::<Vec<_>>()
                .encode(),
            roles_before
        );
        assert!(!mldsa::PqPublicKey::<Runtime>::contains_key(new)); // domain proof does not mutate transaction key registration
    });
}

/// 源码配置清单，不读取或改写任何运行中的链；既有名册不再等待登录时补建。
#[test]
fn institution_wallet_genesis_catalog_has_complete_actual_wallets() {
    use adminbase::{InstitutionAdminQuery, InstitutionAdminWalletQuery};
    new_test_ext().execute_with(|| {
        let mut public_institutions=0;let mut public_wallets=0;
        for (cid,roster) in publicadmin::AdminAccounts::<Runtime>::iter() {
            public_institutions+=1;
            public_wallets+=roster.admins.len();
            for admin in roster.admins {
                assert_eq!(PublicAdmins::resolve_admin_account(roster.institution_code,&cid,&admin.account_id),Some(admin.account_id.clone()));
                assert_eq!(PublicAdmins::resolve_role_authorization(roster.institution_code,&cid,&admin.account_id),Some(admin.account_id.clone()));
                assert_eq!(PublicAdmins::institution_wallet_state(roster.institution_code,&cid,&admin.account_id),Some(adminbase::InstitutionAdminWalletState{wallet:Some(admin.account_id),revision:0}));
            }
        }
        let mut private_institutions=0;let mut private_wallets=0;
        for (cid,roster) in privateadmin::AdminAccounts::<Runtime>::iter() {
            private_institutions+=1;private_wallets+=roster.admins.len();
            for admin in roster.admins {
                assert_eq!(PrivateAdmins::resolve_admin_account(roster.institution_code,&cid,&admin.account_id),Some(admin.account_id.clone()));
                assert_eq!(PrivateAdmins::resolve_role_authorization(roster.institution_code,&cid,&admin.account_id),Some(admin.account_id.clone()));
                assert_eq!(PrivateAdmins::institution_wallet_state(roster.institution_code,&cid,&admin.account_id).unwrap().revision,0);
            }
        }
        assert_eq!(public_institutions,90);assert_eq!(private_institutions,1);
        eprintln!("source genesis admin inventory: public={public_institutions}/{public_wallets} wallets, private={private_institutions}/{private_wallets} wallets");
    });
}

#[test]
fn registry_wallet_six_national_institutions_keep_personnel_and_restore_from_zero() {
    use adminbase::InstitutionAdminQuery;
    new_test_ext().execute_with(|| {
        let actor = test_cid_number(
            primitives::governance_skeleton::federal_registry_institution()
                .cid_number
                .as_bytes(),
        );
        for (index, institution) in primitives::institution_constraints::singleton_institutions()
            .into_iter()
            .enumerate()
        {
            let cid = test_cid_number(institution.cid_number.as_bytes());
            let (province, _) = primitives::cid::number::cid_scope_codes(&cid).unwrap();
            let role: publicentity::RoleCodeOf =
                primitives::governance_skeleton::province_commissioner_role_code(province)
                    .try_into()
                    .unwrap();
            let registrar = publicentity::InstitutionRoleAssignments::<Runtime>::get(&actor, &role)
                [0]
            .account_id
            .clone();
            let personnel = publicentity::InstitutionRoleAssignments::<Runtime>::iter_prefix(&cid)
                .collect::<Vec<_>>()
                .encode();
            let roles = publicentity::InstitutionRoles::<Runtime>::iter_prefix(&cid)
                .collect::<Vec<_>>()
                .encode();
            let accounts = account_state_without_wallet_refs().encode();
            let wallet = AccountId::new([111 + index as u8; 32]);
            let add = || entitybase::InstitutionGovernanceAction::RegisterAdminWalletChanges {
                added: vec![adminbase::Admin {
                    account_id: wallet.clone(),
                    cid_number: Default::default(),
                    family_name: b"F".to_vec().try_into().unwrap(),
                    given_name: b"G".to_vec().try_into().unwrap(),
                }],
                removed: vec![],
                replaced: vec![],
            };
            let send = |action| {
                PublicManage::register_institution_admins(
                    RuntimeOrigin::signed(registrar.clone()),
                    cid.clone(),
                    action,
                    actor.to_vec(),
                    role.to_vec(),
                    PublicManage::registration_state_hash(&cid).unwrap(),
                    None,
                )
            };
            assert_ok!(send(add()));
            assert_eq!(
                PublicAdmins::institution_admins_len(institution.code, &cid),
                Some(1)
            );
            assert_ok!(send(
                entitybase::InstitutionGovernanceAction::RegisterAdminWalletChanges {
                    added: vec![],
                    removed: vec![(wallet.clone(), 0)],
                    replaced: vec![]
                }
            ));
            assert_eq!(
                PublicAdmins::institution_admins_len(institution.code, &cid),
                Some(0)
            );
            assert_ok!(send(add()));
            assert_eq!(
                publicadmin::AdminWallets::<Runtime>::get(&cid, &wallet)
                    .unwrap()
                    .revision,
                2
            );
            assert_eq!(
                publicentity::InstitutionRoleAssignments::<Runtime>::iter_prefix(&cid)
                    .collect::<Vec<_>>()
                    .encode(),
                personnel
            );
            assert_eq!(
                publicentity::InstitutionRoles::<Runtime>::iter_prefix(&cid)
                    .collect::<Vec<_>>()
                    .encode(),
                roles
            );
            assert_eq!(
                account_state_without_wallet_refs().encode(),
                accounts
            );
            if let Some(spec) = primitives::institution_constraints::member_composition_by_identity(
                institution.code,
                &cid,
            ) {
                let role_code: publicentity::RoleCodeOf =
                    spec.role_code.to_vec().try_into().unwrap();
                publicentity::InstitutionRoles::<Runtime>::insert(
                    &cid,
                    &role_code,
                    entitybase::InstitutionRole {
                        cid_number: cid.clone(),
                        role_code: role_code.clone(),
                        role_name: spec.role_name.to_vec().try_into().unwrap(),
                        term_required: false,
                        role_status: entitybase::InstitutionRoleStatus::Active,
                    },
                );
                let members = (0..spec.min_members)
                    .map(|i| {
                        let mut raw = [0xf0; 32];
                        raw[..4].copy_from_slice(&i.to_le_bytes());
                        AccountId::new(raw)
                    })
                    .collect::<Vec<_>>();
                assert_ok!(PublicManage::apply_institution_governance_result(
                    entitybase::InstitutionGovernanceResult {
                        institution_code: institution.code,
                        cid_number: cid.to_vec(),
                        proposal_id: 123,
                        role_mutations: vec![],
                        assignment_changes: vec![entitybase::InstitutionRoleAssignmentChange {
                            role_code: spec.role_code.to_vec(),
                            assignments: members
                                .iter()
                                .cloned()
                                .map(|account_id| entitybase::InstitutionAssignmentTarget {
                                    account_id,
                                    term_start: 0,
                                    term_end: 0,
                                    assignment_source:
                                        entitybase::InstitutionAssignmentSource::PopularElection,
                                    assignment_source_ref: b"election-result".to_vec(),
                                    assignment_status:
                                        entitybase::InstitutionAssignmentStatus::Active
                                })
                                .collect()
                        }],
                        legal_representative_change: None,
                        result_source_ref: b"election-result".to_vec()
                    }
                ));
                assert_eq!(
                    PublicAdmins::institution_admins_len(institution.code, &cid),
                    Some(1)
                );
                assert!(
                    PublicAdmins::resolve_admin_account(institution.code, &cid, &members[0])
                        .is_none()
                );
            }
        }
    });
}

#[test]
fn registry_wallet_frg_replacement_checks_original_province_after_each_rotation() {
    use adminbase::InstitutionAdminQuery;
    new_test_ext().execute_with(|| {
        let cid = test_cid_number(
            primitives::governance_skeleton::federal_registry_institution()
                .cid_number
                .as_bytes(),
        );
        let role: publicentity::RoleCodeOf =
            primitives::governance_skeleton::province_commissioner_role_code(*b"GD")
                .try_into()
                .unwrap();
        let other: publicentity::RoleCodeOf =
            primitives::governance_skeleton::province_commissioner_role_code(*b"HU")
                .try_into()
                .unwrap();
        let group = publicentity::InstitutionRoleAssignments::<Runtime>::get(&cid, &role);
        let outside = publicentity::InstitutionRoleAssignments::<Runtime>::get(&cid, &other)[0]
            .account_id
            .clone();
        let registrar = group[0].account_id.clone();
        let target = group[1].account_id.clone();
        let before = publicentity::InstitutionRoleAssignments::<Runtime>::iter_prefix(&cid)
            .collect::<Vec<_>>()
            .encode();
        let subject = engingcore::AuthorizationSubject::Institution(engingcore::RoleSubject {
            cid_number: cid.to_vec().try_into().unwrap(),
            role_code: role.to_vec().try_into().unwrap(),
        });
        let frozen: frame_support::BoundedVec<
            AccountId,
            <Runtime as engingcore::Config>::MaxAdminsPerInstitution,
        > = group
            .iter()
            .map(|a| a.account_id.clone())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        engingcore::VoterSnapshot::<Runtime>::insert(9988128u128, &subject, &frozen);

        let send = |action| {
            PublicManage::register_institution_admins(
                RuntimeOrigin::signed(registrar.clone()),
                cid.clone(),
                action,
                cid.to_vec(),
                role.to_vec(),
                PublicManage::registration_state_hash(&cid).unwrap(),
                None,
            )
        };
        let action = |old: AccountId, new: AccountId| {
            entitybase::InstitutionGovernanceAction::RegisterAdminWalletChanges {
                added: vec![],
                removed: vec![],
                replaced: vec![adminbase::InstitutionAdminWalletChange {
                    admin_account: old.clone(),
                    expected_wallet: old,
                    new_wallet: new,
                    expected_revision: 0,
                }],
            }
        };
        assert_noop!(
            send(action(outside, AccountId::new([191; 32]))),
            publicentity::Error::<Runtime>::PermissionDenied
        );
        let new = AccountId::new([192; 32]);
        assert_ok!(send(action(target.clone(), new.clone())));
        assert_ok!(send(action(new.clone(), AccountId::new([193; 32]))));
        assert_eq!(
            PublicAdmins::institution_admins_len(adminbase::FRG, &cid),
            Some(215)
        );
        assert_eq!(
            publicentity::InstitutionRoleAssignments::<Runtime>::iter_prefix(&cid)
                .collect::<Vec<_>>()
                .encode(),
            before
        );
        assert_eq!(
            engingcore::VoterSnapshot::<Runtime>::get(9988128, &subject),
            Some(frozen)
        );
        assert_eq!(
            publicadmin::WalletRoleBindings::<Runtime>::get(&cid, AccountId::new([193; 32])),
            Some(target)
        );
    });
}

#[test]
fn frozen_institution_votes_survive_real_frg_wallet_rotation_without_expanding_current_authority() {
    use adminbase::InstitutionAdminQuery;
    use codec::Encode;
    use frame_support::traits::Hooks;
    new_test_ext().execute_with(|| {
        let cid = test_cid_number(
            primitives::governance_skeleton::federal_registry_institution()
                .cid_number
                .as_bytes(),
        );
        let role: publicentity::RoleCodeOf =
            primitives::governance_skeleton::province_commissioner_role_code(*b"GD")
                .try_into()
                .unwrap();
        let group = publicentity::InstitutionRoleAssignments::<Runtime>::get(&cid, &role)
            .iter()
            .map(|a| a.account_id.clone())
            .collect::<Vec<_>>();
        let personnel = publicentity::InstitutionRoleAssignments::<Runtime>::iter_prefix(&cid)
            .collect::<Vec<_>>()
            .encode();
        let accounts = account_state_without_wallet_refs().encode();
        let expires_at = 1_782_950_400 + 86400;
        let action = |old: AccountId, seed: u8| {
            let pair = sr25519::Pair::from_seed(&[seed; 32]);
            let change = adminbase::InstitutionAdminWalletChange {
                admin_account: old.clone(),
                expected_wallet: old,
                new_wallet: AccountId::new(pair.public().0),
                expected_revision: 0,
            };
            let acceptance = adminbase::InstitutionWalletAcceptance {
                genesis_hash: System::block_hash(0),
                cid_number: cid.to_vec().try_into().unwrap(),
                change: change.clone(),
                mode: adminbase::InstitutionWalletChangeMode::Governance,
                expires_at,
            };
            let proof = pair
                .sign(&primitives::sign::signing_message(
                    primitives::sign::OP_SIGN_INSTITUTION_WALLET,
                    &acceptance.encode(),
                ))
                .0
                .to_vec()
                .try_into()
                .unwrap();
            (
                entitybase::InstitutionGovernanceAction::RotateAdminWallet {
                    change: change.clone(),
                    expires_at,
                    new_wallet_proof: proof,
                },
                change,
            )
        };
        let (first, target) = action(group[4].clone(), 219);
        assert_ok!(PublicManage::propose_institution_governance(
            RuntimeOrigin::signed(group[0].clone()),
            cid.clone(),
            first,
            cid.to_vec(),
            role.clone()
        ));
        let pid = VotingEngine::next_proposal_id() - 1;
        let plan = VotingEngine::proposal_vote_plan(pid).unwrap();
        let original =
            engingcore::VoterSnapshot::<Runtime>::get(pid, &plan.proposer_subject).unwrap();
        let versions =
            engingcore::FrozenWalletVersions::<Runtime>::get(pid, &plan.proposer_subject).unwrap();
        let first_pair = sr25519::Pair::from_seed(&[220; 32]);
        let new_creator = AccountId::new(first_pair.public().0);
        let second_pair = sr25519::Pair::from_seed(&[221; 32]);
        let new_voter = AccountId::new(second_pair.public().0);
        let changes = vec![
            (group[0].clone(), new_creator.clone()),
            (group[1].clone(), new_voter.clone()),
        ]
        .into_iter()
        .map(
            |(old, new_wallet)| adminbase::InstitutionAdminWalletChange {
                admin_account: old.clone(),
                expected_wallet: old,
                new_wallet,
                expected_revision: 0,
            },
        )
        .collect();
        assert_ok!(PublicManage::register_institution_admins(
            RuntimeOrigin::signed(group[2].clone()),
            cid.clone(),
            entitybase::InstitutionGovernanceAction::RegisterAdminWalletChanges {
                added: vec![],
                removed: vec![],
                replaced: changes
            },
            cid.to_vec(),
            role.to_vec(),
            PublicManage::registration_state_hash(&cid).unwrap(),
            None
        ));
        assert!(PublicAdmins::resolve_admin_account(adminbase::FRG, &cid, &group[1]).is_none());
        let claim = || internalvote::InternalVoteTicketClaim::InstitutionRole(role.clone());
        assert_noop!(
            InternalVote::cast(RuntimeOrigin::signed(new_voter.clone()), pid, claim(), true),
            engingcore::Error::<Runtime>::NoPermission
        );
        assert_noop!(
            InternalVote::cast(RuntimeOrigin::signed(group[0].clone()), pid, claim(), false),
            engingcore::Error::<Runtime>::AlreadyVoted
        );
        assert_eq!(internalvote::InternalTallies::<Runtime>::get(pid).yes, 1);
        let vote_call = RuntimeCall::InternalVote(internalvote::Call::cast {
            proposal_id: pid,
            ticket_claim: claim(),
            approve: true,
        });
        assert_eq!(
            <RuntimeFeeRouter as onchain::CallFeeRoute<AccountId, RuntimeCall, Balance>>::fee_route(
                &group[1], &vote_call
            ),
            primitives::fee_policy::FeeRoute::Vote {
                payer_account_id: group[1].clone()
            }
        );
        assert_ok!(InternalVote::cast(
            RuntimeOrigin::signed(group[1].clone()),
            pid,
            claim(),
            true
        ));
        assert_ok!(InternalVote::cast(
            RuntimeOrigin::signed(group[2].clone()),
            pid,
            claim(),
            true
        ));
        <VotingEngine as Hooks<BlockNumber>>::on_initialize(System::block_number());
        assert_eq!(
            publicadmin::AdminWallets::<Runtime>::get(&cid, &target.new_wallet)
                .unwrap()
                .wallet,
            Some(target.new_wallet)
        );
        assert_eq!(
            engingcore::VoterSnapshot::<Runtime>::get(pid, &plan.proposer_subject),
            Some(original)
        );
        assert_eq!(
            engingcore::FrozenWalletVersions::<Runtime>::get(pid, &plan.proposer_subject),
            Some(versions)
        );
        let (next, _) = action(group[3].clone(), 222);
        assert_ok!(PublicManage::propose_institution_governance(
            RuntimeOrigin::signed(new_creator.clone()),
            cid.clone(),
            next,
            cid.to_vec(),
            role.clone()
        ));
        let fresh = VotingEngine::next_proposal_id() - 1;
        let fresh_plan = VotingEngine::proposal_vote_plan(fresh).unwrap();
        assert_eq!(
            VotingEngine::frozen_voter_identity(fresh, &fresh_plan.proposer_subject, &new_voter),
            Some((new_voter, 0))
        );
        assert_noop!(
            InternalVote::cast(
                RuntimeOrigin::signed(group[1].clone()),
                fresh,
                claim(),
                true
            ),
            engingcore::Error::<Runtime>::NoPermission
        );
        assert_eq!(
            publicentity::InstitutionRoleAssignments::<Runtime>::iter_prefix(&cid)
                .collect::<Vec<_>>()
                .encode(),
            personnel
        );
        assert_eq!(
            account_state_without_wallet_refs().encode(),
            accounts
        );
        assert_eq!(
            PublicAdmins::institution_admins_len(adminbase::FRG, &cid),
            Some(215)
        );
    });
}

#[test]
fn missing_current_wallet_data_cannot_shrink_frozen_role_population() {
    use codec::Encode;
    new_test_ext().execute_with(|| {
        let cid=test_cid_number(primitives::governance_skeleton::federal_registry_institution().cid_number.as_bytes());
        let role:publicentity::RoleCodeOf=primitives::governance_skeleton::province_commissioner_role_code(*b"GD").try_into().unwrap();
        let group=publicentity::InstitutionRoleAssignments::<Runtime>::get(&cid,&role).iter().map(|a|a.account_id.clone()).collect::<Vec<_>>();
        let pair=sr25519::Pair::from_seed(&[223;32]);let expires_at=1_782_950_400+86400;
        let change=adminbase::InstitutionAdminWalletChange{admin_account:group[3].clone(),expected_wallet:group[3].clone(),new_wallet:AccountId::new(pair.public().0),expected_revision:0};
        let acceptance=adminbase::InstitutionWalletAcceptance{genesis_hash:System::block_hash(0),cid_number:cid.to_vec().try_into().unwrap(),change:change.clone(),mode:adminbase::InstitutionWalletChangeMode::Governance,expires_at};
        let proof=pair.sign(&primitives::sign::signing_message(primitives::sign::OP_SIGN_INSTITUTION_WALLET,&acceptance.encode())).0.to_vec().try_into().unwrap();
        publicadmin::AdminWallets::<Runtime>::remove(&cid,&group[4]);
        assert_eq!(<RuntimeInstitutionRoleProvider as engingcore::InstitutionRoleProvider<AccountId>>::active_accounts_for_role(&cid,&role).len(),5);
        let next=VotingEngine::next_proposal_id();
        assert!(PublicManage::propose_institution_governance(RuntimeOrigin::signed(group[0].clone()),cid.clone(),entitybase::InstitutionGovernanceAction::RotateAdminWallet{change,expires_at,new_wallet_proof:proof},cid.to_vec(),role).is_err());
        assert_eq!(VotingEngine::next_proposal_id(),next);
        assert!(!engingcore::VoterSnapshot::<Runtime>::iter_prefix(next).next().is_some());
        assert!(!engingcore::FrozenWalletVersions::<Runtime>::iter_prefix(next).next().is_some());
    });
}

#[test]
fn frozen_wallet_metadata_does_not_change_existing_snapshot_or_vote_ticket_codec() {
    use codec::Encode;
    let f: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/primitives/tests/fixtures/role_permission.json"
    )))
    .unwrap();
    let f = &f["frozen_institution_voters"];
    let cid = f["cid_number"].as_str().unwrap().as_bytes();
    let role = f["role_code"].as_str().unwrap().as_bytes();
    let records = vec![([17u8; 32], 0u64), ([34u8; 32], 6u64)];
    let ticket = engingcore::types::InstitutionVoteTicket::<[u8; 32]> {
        role_subject: engingcore::types::RoleSubject {
            cid_number: cid.to_vec().try_into().unwrap(),
            role_code: role.to_vec().try_into().unwrap(),
        },
        voter_account_id: [17u8; 32],
    };
    let hex = |bytes: Vec<u8>| {
        format!(
            "0x{}",
            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
        )
    };
    assert_eq!(hex(vec![[17u8; 32], [34u8; 32]].encode()), f["voters_hex"]);
    assert_eq!(hex(records.encode()), f["versions_hex"]);
    assert_eq!(hex(ticket.encode()), f["original_ticket_hex"]);
}

/// 完整V5认证及交易扩展：零余额新钱包由机构费用账户支付，退役再登记不重置nonce。
#[test]
fn institution_wallet_zero_balance_executes_native_pipeline_and_never_reuses_nonce() {
    use adminbase::{InstitutionAdminWalletLifecycle, InstitutionAdminWalletQuery};
    use entitybase::InstitutionMultisigQuery;
    use frame_support::traits::Currency;
    use sp_runtime::traits::{ExtensionVariant, PipelineAtVers, TransactionExtension};
    new_test_ext().execute_with(|| {
        let cid = test_cid_number(
            primitives::governance_skeleton::federal_registry_institution()
                .cid_number
                .as_bytes(),
        );
        let old = PublicAdmins::institution_admins(adminbase::FRG, &cid).unwrap()[0].clone();
        let seed = [0x91; 32];
        let public = mldsa::public_key(&seed);
        let account = mldsa::account_id(&public);
        let wallet = AccountId::new(account);
        assert_eq!(System::account(&wallet).providers, 0);
        assert_eq!(System::account(&wallet).sufficients, 0);
        assert!(
            frame_system::CheckNonce::<Runtime>::validate_nonce_for_account(&wallet, 0).is_err()
        );
        assert_ok!(PublicAdmins::change_institution_wallet(
            adminbase::FRG,
            cid.to_vec(),
            adminbase::InstitutionAdminWalletChange {
                admin_account: old.clone(),
                expected_wallet: old,
                new_wallet: wallet.clone(),
                expected_revision: 0
            }
        ));
        assert_eq!(Balances::free_balance(&wallet), 0);
        assert_eq!(System::account(&wallet).providers, 0);
        assert_eq!(System::account(&wallet).sufficients, 1);
        assert!(
            frame_system::CheckNonce::<Runtime>::validate_nonce_for_account(&wallet, 0).is_ok()
        );
        let target_seed = [0x92; 32];
        let target_public = mldsa::public_key(&target_seed);
        let target = AccountId::new(mldsa::account_id(&target_public));
        let change = adminbase::InstitutionAdminWalletChange {
            admin_account: wallet.clone(),
            expected_wallet: wallet.clone(),
            new_wallet: target.clone(),
            expected_revision: 0,
        };
        let expires_at = 1_782_950_400 + 180;
        let acceptance = adminbase::InstitutionWalletAcceptance {
            genesis_hash: System::block_hash(0),
            cid_number: cid.to_vec().try_into().unwrap(),
            change: change.clone(),
            mode: adminbase::InstitutionWalletChangeMode::SelfSigned,
            expires_at,
        };
        let accept = mldsa::sign(
            &target_seed,
            &primitives::sign::signing_message(
                primitives::sign::OP_SIGN_INSTITUTION_WALLET,
                &acceptance.encode(),
            ),
        )
        .unwrap();
        let call = RuntimeCall::PublicManage(publicentity::Call::rotate_institution_admin_wallet {
            cid_number: cid.clone(),
            change,
            expires_at,
            new_wallet_proof: mldsa::encode_pq_signature(&target_public, &accept)
                .try_into()
                .unwrap(),
        });
        let fee_account = RuntimeInstitutionQuery::lookup_institution_account(
            &cid,
            primitives::account_derive::RESERVED_NAME_FEE,
        )
        .unwrap();
        let _ = Balances::make_free_balance_be(&fee_account, 10_000_000);
        <mldsa::GenesisChainPhase<Runtime> as mldsa::ChainPhase>::enter_operation();
        let ext: TxExtension = (
            frame_system::AuthorizeCall::new(),
            frame_system::CheckNonZeroSender::new(),
            CheckNonStakeSender,
            frame_system::CheckSpecVersion::new(),
            frame_system::CheckTxVersion::new(),
            frame_system::CheckGenesis::new(),
            frame_system::CheckEra::from(sp_runtime::generic::Era::Immortal),
            frame_system::CheckNonce::from(0),
            frame_system::CheckWeight::new(),
            pallet_transaction_payment::ChargeTransactionPayment::from(0),
            frame_metadata_hash_extension::CheckMetadataHash::new(false),
            (Default::default(), frame_system::WeightReclaim::new()),
        );
        let next_ext_template = ext.clone();
        let review = (call.clone(), ext.clone(), ext.implicit().unwrap()).encode();
        let mut implication = vec![mldsa::EXTENSION_VERSION];
        implication.extend(review);
        let proof = mldsa::Proof::First {
            account,
            public,
            signature: mldsa::sign(&seed, &mldsa::transaction_message(&account, &implication))
                .unwrap(),
        };
        let xt: UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic::from_parts(
            call,
            sp_runtime::generic::Preamble::General(ExtensionVariant::Other(
                PipelineAtVers::<1, _>::new((mldsa::Authenticate::<Runtime>::from(proof), ext)),
            )),
        )
        .into();
        let fees_before = Balances::free_balance(&fee_account);
        assert_ok!(Executive::apply_extrinsic(xt.clone())
            .expect("full native pipeline accepts funded institution and zero balance signer"));
        assert_eq!(Balances::free_balance(&wallet), 0);
        assert!(Balances::free_balance(&fee_account) < fees_before);
        assert_eq!(System::account_nonce(&wallet), 1);
        assert_eq!(System::account(&wallet).sufficients, 1);
        assert_eq!(
            PublicAdmins::current_institution_wallet(adminbase::FRG, &cid, &target),
            Some(target.clone())
        );
        assert_ok!(PublicAdmins::change_institution_wallet(
            adminbase::FRG,
            cid.to_vec(),
            adminbase::InstitutionAdminWalletChange {
                admin_account: target.clone(),
                expected_wallet: target,
                new_wallet: wallet.clone(),
                expected_revision: 0
            }
        ));
        assert_eq!(System::account_nonce(&wallet), 1);
        assert_eq!(System::account(&wallet).sufficients, 1);
        assert_eq!(
            PublicAdmins::institution_wallet_state(adminbase::FRG, &cid, &wallet)
                .unwrap()
                .revision,
            2
        );
        assert!(
            frame_system::CheckNonce::<Runtime>::validate_nonce_for_account(&wallet, 0).is_err()
        );
        assert!(Executive::apply_extrinsic(xt).is_err());
        assert_eq!(Balances::free_balance(&wallet), 0);
        // 机构费用账户有余额也不能代付原定额投票费；完整扩展在dispatch前拒绝。
        let vote = RuntimeCall::InternalVote(internalvote::Call::cast {
            proposal_id: 0,
            ticket_claim: internalvote::InternalVoteTicketClaim::InstitutionRole(
                b"ORIGINAL_ROLE".to_vec().try_into().unwrap(),
            ),
            approve: true,
        });
        let mut vote_ext = next_ext_template;
        vote_ext.7 = frame_system::CheckNonce::from(1);
        let mut implication = vec![mldsa::EXTENSION_VERSION];
        implication.extend((vote.clone(), vote_ext.clone(), vote_ext.implicit().unwrap()).encode());
        let proof = mldsa::Proof::Stored {
            account,
            signature: mldsa::sign(&seed, &mldsa::transaction_message(&account, &implication))
                .unwrap(),
        };
        let vote_xt: UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic::from_parts(
            vote,
            sp_runtime::generic::Preamble::General(ExtensionVariant::Other(
                PipelineAtVers::<1, _>::new((
                    mldsa::Authenticate::<Runtime>::from(proof),
                    vote_ext,
                )),
            )),
        )
        .into();
        assert!(matches!(
            Executive::apply_extrinsic(vote_xt),
            Err(
                sp_runtime::transaction_validity::TransactionValidityError::Invalid(
                    sp_runtime::transaction_validity::InvalidTransaction::Payment
                )
            )
        ));
        assert_eq!(System::account_nonce(&wallet), 1);
    });
}

#[test]
fn frg_retired_roster_actions_and_cross_province_personnel_are_rejected() {
    new_test_ext().execute_with(|| {
        let institution = primitives::governance_skeleton::federal_registry_institution();
        let cid = test_cid_number(institution.cid_number.as_bytes());
        let role: publicentity::RoleCodeOf =
            primitives::governance_skeleton::province_commissioner_role_code(*b"GD")
                .try_into()
                .unwrap();
        let other: publicentity::RoleCodeOf =
            primitives::governance_skeleton::province_commissioner_role_code(*b"HU")
                .try_into()
                .unwrap();
        let group = publicentity::InstitutionRoleAssignments::<Runtime>::get(&cid, &role);
        let registrar = group[0].account_id.clone();
        let admins = publicadmin::AdminAccounts::<Runtime>::get(&cid)
            .unwrap()
            .admins;
        assert_eq!(admins.len(), 215);
        let original = publicentity::InstitutionRoleAssignments::<Runtime>::iter_prefix(&cid)
            .collect::<Vec<_>>()
            .encode();
        let mut target = group.clone();
        target[1].account_id = AccountId::new([242; 32]);
        let change = entitybase::InstitutionRoleAssignmentChange {
            role_code: role.to_vec(),
            assignments: target
                .iter()
                .map(|a| entitybase::InstitutionAssignmentTarget {
                    account_id: a.account_id.clone(),
                    term_start: 0,
                    term_end: 0,
                    assignment_source:
                        entitybase::InstitutionAssignmentSource::InstitutionGovernance,
                    assignment_source_ref: vec![],
                    assignment_status: entitybase::InstitutionAssignmentStatus::Active,
                })
                .collect(),
        };
        let send = |action| {
            PublicManage::propose_institution_governance(
                RuntimeOrigin::signed(registrar.clone()),
                cid.clone(),
                action,
                cid.to_vec(),
                role.clone(),
            )
        };
        for action in [
            entitybase::InstitutionGovernanceAction::ReplaceAdmins {
                admins: admins.to_vec(),
            },
            entitybase::InstitutionGovernanceAction::ReplaceAdminsAndMutateRoles {
                admins: admins.to_vec(),
                role_mutations: vec![],
                assignment_changes: vec![change],
                legal_representative_change: None,
            },
        ] {
            assert_noop!(
                send(action),
                publicentity::Error::<Runtime>::InvalidInstitutionGovernanceAction
            );
        }
        let mut outside = publicentity::InstitutionRoleAssignments::<Runtime>::get(&cid, &other);
        outside[1].account_id = AccountId::new([243; 32]);
        assert_noop!(
            send(
                entitybase::InstitutionGovernanceAction::MutateRolesAndAssignments {
                    role_mutations: vec![],
                    assignment_changes: vec![entitybase::InstitutionRoleAssignmentChange {
                        role_code: other.to_vec(),
                        assignments: outside
                            .iter()
                            .map(|a| entitybase::InstitutionAssignmentTarget {
                                account_id: a.account_id.clone(),
                                term_start: 0,
                                term_end: 0,
                                assignment_source:
                                    entitybase::InstitutionAssignmentSource::InstitutionGovernance,
                                assignment_source_ref: vec![],
                                assignment_status: entitybase::InstitutionAssignmentStatus::Active
                            })
                            .collect()
                    }],
                    legal_representative_change: None,
                }
            ),
            publicentity::Error::<Runtime>::PermissionDenied
        );
        assert_eq!(
            publicadmin::AdminAccounts::<Runtime>::get(&cid)
                .unwrap()
                .admins,
            admins
        );
        assert_eq!(
            publicentity::InstitutionRoleAssignments::<Runtime>::iter_prefix(&cid)
                .collect::<Vec<_>>()
                .encode(),
            original
        );
    });
}


#[test]
fn wallet_upgrade_repairs_existing_91_institutions_and_preserves_all_other_storage() {
    use adminbase::{InstitutionAdminQuery, InstitutionAdminWalletQuery};
    use frame_support::traits::{Currency, Hooks, StorageVersion};
    use std::collections::BTreeMap;
    new_test_ext().execute_with(|| {
        let public = publicadmin::AdminAccounts::<Runtime>::iter().collect::<Vec<_>>();
        let private = privateadmin::AdminAccounts::<Runtime>::iter().collect::<Vec<_>>();
        assert_eq!((public.len(), private.len()), (90, 1));
        let mut references = BTreeMap::<AccountId, u32>::new();
        // 隔离夹具还原实读问题：名单、岗位均已存在，两类钱包记录全为空。
        for (cid, roster) in &public {
            for admin in &roster.admins {
                publicadmin::AdminWallets::<Runtime>::remove(cid, &admin.account_id);
                publicadmin::WalletRoleBindings::<Runtime>::remove(cid, &admin.account_id);
                System::dec_sufficients(&admin.account_id);
                *references.entry(admin.account_id.clone()).or_default() += 1;
            }
        }
        for (cid, roster) in &private {
            for admin in &roster.admins {
                privateadmin::AdminWallets::<Runtime>::remove(cid, &admin.account_id);
                privateadmin::WalletRoleBindings::<Runtime>::remove(cid, &admin.account_id);
                System::dec_sufficients(&admin.account_id);
                *references.entry(admin.account_id.clone()).or_default() += 1;
            }
        }
        assert_eq!(references.values().sum::<u32>(), 1025);
        StorageVersion::new(0).put::<PublicAdmins>();
        StorageVersion::new(0).put::<PrivateAdmins>();
        let sample = public[0].1.admins[0].account_id.clone();
        let _ = Balances::make_free_balance_be(&sample, 12_345);
        frame_system::Account::<Runtime>::mutate(&sample, |info| info.nonce = 23);
        let subject = engingcore::AuthorizationSubject::Institution(engingcore::RoleSubject {
            cid_number: public[0].0.to_vec().try_into().unwrap(),
            role_code: b"COMMITTEE_MEMBER".to_vec().try_into().unwrap(),
        });
        let pid = 998899u128;
        let frozen: frame_support::BoundedVec<
            AccountId,
            <Runtime as engingcore::Config>::MaxAdminsPerInstitution,
        > = vec![sample.clone()].try_into().unwrap();
        engingcore::VoterSnapshot::<Runtime>::insert(pid, &subject, &frozen);
        internalvote::InternalTallies::<Runtime>::insert(
            pid,
            engingcore::VoteCountU32 { yes: 7, no: 2 },
        );
        let engingcore::AuthorizationSubject::Institution(role_subject) = subject else {
            unreachable!()
        };
        internalvote::InternalVotesByTicket::<Runtime>::insert(
            pid,
            internalvote::InternalVoteTicket::Institution(engingcore::InstitutionVoteTicket {
                role_subject,
                voter_account_id: sample.clone(),
            }),
            true,
        );
        let mut expected_accounts =
            frame_system::Account::<Runtime>::iter().collect::<BTreeMap<_, _>>();
        for (wallet, count) in &references {
            expected_accounts
                .entry(wallet.clone())
                .or_default()
                .sufficients += *count;
        }
        let mut allowed = Vec::new();
        for pallet in [b"PublicAdmins".as_slice(), b"PrivateAdmins".as_slice()] {
            for name in [b"AdminWallets".as_slice(), b"WalletRoleBindings".as_slice()] {
                allowed.push(frame_support::storage::storage_prefix(pallet, name).to_vec());
            }
        }
        allowed.push(StorageVersion::storage_key::<PublicAdmins>().to_vec());
        allowed.push(StorageVersion::storage_key::<PrivateAdmins>().to_vec());
        for name in [
            b"Account".as_slice(),
            b"Events".as_slice(),
            b"EventCount".as_slice(),
        ] {
            allowed.push(frame_support::storage::storage_prefix(b"System", name).to_vec());
        }
        let protected_state = || {
            let mut entries = Vec::new();
            let mut cursor = Vec::new();
            while let Some(key) = sp_io::storage::next_key(&cursor) {
                if !allowed.iter().any(|prefix| key.starts_with(prefix)) {
                    entries.push((key.clone(), sp_io::storage::get(&key).unwrap().to_vec()));
                }
                cursor = key;
            }
            entries
        };
        let before = protected_state();
        let weight = <PublicAdmins as Hooks<BlockNumber>>::on_runtime_upgrade()
            .saturating_add(<PrivateAdmins as Hooks<BlockNumber>>::on_runtime_upgrade());
        assert!(
            weight.all_lte(RuntimeBlockWeights::get().max_block),
            "全部1025条补齐必须在区块预算内"
        );
        assert_eq!(
            StorageVersion::get::<PublicAdmins>(),
            StorageVersion::new(1)
        );
        assert_eq!(
            StorageVersion::get::<PrivateAdmins>(),
            StorageVersion::new(1)
        );
        assert_eq!(
            protected_state(),
            before,
            "名单、岗位、任职、权限、阈值、余额及旧提案所有记录必须逐字节不变"
        );
        assert_eq!(
            frame_system::Account::<Runtime>::iter().collect::<BTreeMap<_, _>>(),
            expected_accounts
        );
        for (cid, roster) in &public {
            for admin in &roster.admins {
                assert_eq!(
                    PublicAdmins::institution_wallet_state(
                        roster.institution_code,
                        cid,
                        &admin.account_id
                    ),
                    Some(adminbase::InstitutionAdminWalletState {
                        wallet: Some(admin.account_id.clone()),
                        revision: 0
                    })
                );
                assert_eq!(
                    PublicAdmins::resolve_role_authorization(
                        roster.institution_code,
                        cid,
                        &admin.account_id
                    ),
                    Some(admin.account_id.clone())
                );
            }
        }
        for (cid, roster) in &private {
            for admin in &roster.admins {
                assert_eq!(
                    PrivateAdmins::institution_wallet_state(
                        roster.institution_code,
                        cid,
                        &admin.account_id
                    ),
                    Some(adminbase::InstitutionAdminWalletState {
                        wallet: Some(admin.account_id.clone()),
                        revision: 0
                    })
                );
                assert_eq!(
                    PrivateAdmins::resolve_role_authorization(
                        roster.institution_code,
                        cid,
                        &admin.account_id
                    ),
                    Some(admin.account_id.clone())
                );
            }
        }
        let after = sp_io::storage::root(sp_runtime::StateVersion::V1);
        <PublicAdmins as Hooks<BlockNumber>>::on_runtime_upgrade();
        <PrivateAdmins as Hooks<BlockNumber>>::on_runtime_upgrade();
        assert_eq!(
            sp_io::storage::root(sp_runtime::StateVersion::V1),
            after,
            "重复升级不能再次补引用或改写状态"
        );
    });
}

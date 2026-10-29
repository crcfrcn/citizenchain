#![cfg(test)]

use super::*;
use frame_support::{
    assert_noop, assert_ok, derive_impl,
    traits::{ConstU128, ConstU32, Currency, ExistenceRequirement, Hooks, WithdrawReasons},
};
use frame_system as system;
use primitives::cid::china::{china_cb::CHINA_CB, china_ch::CHINA_CH};
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};
use votingengine::InstitutionRoleProvider as _;
use votingengine::{STATUS_PASSED, STATUS_REJECTED};

type Balance = u128;
type Block = frame_system::mocking::MockBlock<Test>;

#[frame_support::runtime]
mod runtime {
    #[runtime::runtime]
    #[runtime::derive(
        RuntimeCall,
        RuntimeEvent,
        RuntimeError,
        RuntimeOrigin,
        RuntimeFreezeReason,
        RuntimeHoldReason,
        RuntimeSlashReason,
        RuntimeLockId,
        RuntimeTask,
        RuntimeViewFunction
    )]
    pub struct Test;

    #[runtime::pallet_index(0)]
    pub type System = frame_system;

    #[runtime::pallet_index(1)]
    pub type Balances = pallet_balances;

    #[runtime::pallet_index(2)]
    pub type VotingEngine = votingengine;

    #[runtime::pallet_index(99)]
    pub type InternalVote = internalvote;

    #[runtime::pallet_index(3)]
    pub type ResolutionDestroy = super;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
    type Block = Block;
    type AccountId = AccountId32;
    type Lookup = IdentityLookup<Self::AccountId>;
    type AccountData = pallet_balances::AccountData<Balance>;
}

impl pallet_balances::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Balance = Balance;
    type DustRemoval = ();
    type ExistentialDeposit = ConstU128<1>;
    type AccountStore = System;
    type MaxLocks = ConstU32<0>;
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type FreezeIdentifier = RuntimeFreezeReason;
    type MaxFreezes = ConstU32<0>;
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type DoneSlashHandler = ();
    type WeightInfo = ();
}

pub struct TestCitizenIdentityReader;
impl votingengine::CitizenIdentityReader<AccountId32> for TestCitizenIdentityReader {
    fn voting_subject(
        who: &AccountId32,
        _scope: &votingengine::PopulationScope,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        Some(test_citizen_subject(who))
    }

    fn candidate_subject(
        who: &AccountId32,
        _scope: &votingengine::PopulationScope,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        Some(test_citizen_subject(who))
    }
}

fn test_citizen_subject(who: &AccountId32) -> votingengine::CitizenSubject<AccountId32> {
    votingengine::CitizenSubject {
        cid_number: <AccountId32 as AsRef<[u8]>>::as_ref(who)
            .to_vec()
            .try_into()
            .expect("account fits CID"),
        account_id: who.clone(),
    }
}

pub struct TestInternalAdminProvider;

impl TestInternalAdminProvider {
    fn institution_admins(
        institution_code: InstitutionCode,
        cid_number: &[u8],
    ) -> Option<sp_std::vec::Vec<AccountId32>> {
        match institution_code {
            NRC | PRC => CHINA_CB
                .iter()
                .find(|n| n.cid_number.as_bytes() == cid_number)
                .map(|n| n.admins.iter().copied().map(AccountId32::new).collect()),
            PRB => CHINA_CH
                .iter()
                .find(|n| n.cid_number.as_bytes() == cid_number)
                .map(|n| n.admins.iter().copied().map(AccountId32::new).collect()),
            _ => None,
        }
    }
}

impl votingengine::InternalAdminProvider<AccountId32> for TestInternalAdminProvider {
    fn is_institution_admin(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        who: &AccountId32,
    ) -> bool {
        Self::institution_admins(institution_code, cid_number)
            .map(|admins| admins.contains(who))
            .unwrap_or(false)
    }

    fn institution_threshold(institution_code: InstitutionCode, cid_number: &[u8]) -> Option<u32> {
        Self::institution_admins(institution_code, cid_number)?;
        primitives::cid::code::fixed_governance_pass_threshold(&institution_code)
    }
}

pub struct TestInstitutionRoleProvider;

fn test_role_code(institution_code: InstitutionCode) -> Option<&'static [u8]> {
    match institution_code {
        NRC | PRC => Some(primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER),
        PRB => Some(primitives::governance_skeleton::ROLE_CODE_DIRECTOR),
        _ => None,
    }
}

fn bounded_test_role(institution_code: InstitutionCode) -> votingengine::types::RoleCode {
    test_role_code(institution_code)
        .unwrap_or(primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER)
        .to_vec()
        .try_into()
        .expect("test role fits protocol bound")
}

impl votingengine::InstitutionRoleProvider<AccountId32> for TestInstitutionRoleProvider {
    fn is_active_assignment(cid_number: &[u8], who: &AccountId32, role_code: &[u8]) -> bool {
        Self::active_accounts_for_role(cid_number, role_code).contains(who)
    }

    fn active_accounts_for_role(cid_number: &[u8], role_code: &[u8]) -> Vec<AccountId32> {
        let Some(code) = core::str::from_utf8(cid_number)
            .ok()
            .and_then(votingengine::types::institution_code_from_cid_number)
        else {
            return Vec::new();
        };
        if test_role_code(code) != Some(role_code) {
            return Vec::new();
        }
        TestInternalAdminProvider::institution_admins(code, cid_number).unwrap_or_default()
    }
}

impl entitybase::InstitutionRoleAuthorizationQuery<AccountId32> for TestInstitutionRoleProvider {
    fn role_has_permission(
        role_subject: &entitybase::RoleSubject<Vec<u8>, Vec<u8>>,
        business_action_id: &entitybase::BusinessActionId<Vec<u8>>,
        _operation: entitybase::RolePermissionOperation,
    ) -> bool {
        let Some(code) = core::str::from_utf8(role_subject.cid_number.as_slice())
            .ok()
            .and_then(votingengine::types::institution_code_from_cid_number)
        else {
            return false;
        };
        test_role_code(code) == Some(role_subject.role_code.as_slice())
            && business_action_id.module_tag.as_slice() == crate::MODULE_TAG
            && business_action_id.action_code
                == entitybase::business_action::ACTION_RESOLUTION_DESTROY
    }

    fn is_authorized(
        admin: &AccountId32,
        role_subject: &entitybase::RoleSubject<Vec<u8>, Vec<u8>>,
        business_action_id: &entitybase::BusinessActionId<Vec<u8>>,
        operation: entitybase::RolePermissionOperation,
    ) -> bool {
        Self::role_has_permission(role_subject, business_action_id, operation)
            && Self::is_active_assignment(
                role_subject.cid_number.as_slice(),
                admin,
                role_subject.role_code.as_slice(),
            )
    }

    fn role_subjects_with_permission(
        cid_number: &[u8],
        business_action_id: &entitybase::BusinessActionId<Vec<u8>>,
        operation: entitybase::RolePermissionOperation,
    ) -> Vec<entitybase::RoleSubject<Vec<u8>, Vec<u8>>> {
        let Some(code) = core::str::from_utf8(cid_number)
            .ok()
            .and_then(votingengine::types::institution_code_from_cid_number)
        else {
            return Vec::new();
        };
        let Some(role_code) = test_role_code(code) else {
            return Vec::new();
        };
        let role_subject = entitybase::RoleSubject {
            cid_number: cid_number.to_vec(),
            role_code: role_code.to_vec(),
        };
        Self::role_has_permission(&role_subject, business_action_id, operation)
            .then_some(role_subject)
            .into_iter()
            .collect()
    }
}

pub struct TestTimeProvider;

impl frame_support::traits::UnixTime for TestTimeProvider {
    fn now() -> core::time::Duration {
        core::time::Duration::from_secs(1_782_864_000) // 2026-07-01
    }
}

impl votingengine::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxVoteNonceLength = ConstU32<64>;
    type MaxVoteSignatureLength = ConstU32<64>;
    type MaxAdminsPerInstitution = ConstU32<32>;
    type MaxAutoFinalizePerBlock = ConstU32<64>;
    type MaxAutoFinalizeWeightPerBlock = votingengine::BlockWeightFraction<Test, 4>;
    type MaxExecutionWeightPerBlock = votingengine::BlockWeightFraction<Test, 4>;
    type MaxCleanupWeightPerBlock = votingengine::BlockWeightFraction<Test, 8>;
    type MaxProposalsPerExpiry = ConstU32<128>;
    type MaxInternalProposalMutexBindings = ConstU32<256>;
    type MaxActiveProposals = ConstU32<10>;
    type MaxCleanupStepsPerBlock = ConstU32<8>;
    type CleanupKeysPerStep = ConstU32<64>;
    type MaxProposalDataLen = ConstU32<256>;
    type MaxProposalObjectLen = ConstU32<{ 10 * 1024 }>;
    type MaxModuleTagLen = ConstU32<32>;
    type MaxManualExecutionAttempts = ConstU32<3>;
    type ExecutionRetryGraceBlocks = frame_support::traits::ConstU64<216>;
    type MaxExecutionRetryDeadlinesPerBlock = ConstU32<128>;
    type MaxCleanupActivationsPerBlock = ConstU32<50>;
    type MaxPendingRetryExpirationsPerBlock = ConstU32<16>;
    type CitizenIdentityReader = TestCitizenIdentityReader;
    type JointVoteResultCallback = ();
    // 挂上本模块 Executor,让提案通过后自动触发销毁执行。
    type InternalVoteResultCallback = crate::InternalVoteExecutor<Test>;
    type InternalAdminProvider = TestInternalAdminProvider;
    type TimeProvider = TestTimeProvider;
    type WeightInfo = ();
    type TrackHandlers = (InternalVote, ());
    type LegislationVoteResultCallback = ();
    type ElectionVoteResultCallback = ();
}

impl internalvote::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = TestInstitutionRoleProvider;
    type WeightInfo = ();
}

impl pallet::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type InternalVoteEngine = internalvote::Pallet<Test>;
    type InstitutionRoleAuthorization = TestInstitutionRoleProvider;
    type InstitutionQuery = TestInstitutionQuery;
    type OnchainFeeCharger = TestOnchainFeeCharger;
    type WeightInfo = ();
}

/// 销毁执行测试真实按统一公式从机构费用账户扣除链上交易费。
pub struct TestOnchainFeeCharger;
impl primitives::fee_policy::OnchainFeeCharger<AccountId32, Balance> for TestOnchainFeeCharger {
    fn charge(
        payer_account_id: &AccountId32,
        transaction_amount: Balance,
    ) -> Result<Balance, sp_runtime::DispatchError> {
        let fee = primitives::fee_policy::calculate_onchain_fee(transaction_amount);
        let imbalance = Balances::withdraw(
            payer_account_id,
            fee,
            WithdrawReasons::FEE,
            ExistenceRequirement::KeepAlive,
        )?;
        drop(imbalance);
        Ok(fee)
    }
}

pub struct TestInstitutionQuery;
impl entitybase::InstitutionMultisigQuery<AccountId32> for TestInstitutionQuery {
    fn lookup_institution_account(cid_number: &[u8], account_name: &[u8]) -> Option<AccountId32> {
        if account_name == primitives::account_derive::RESERVED_NAME_MAIN {
            return test_institution_by_cid(cid_number).map(|(_, main_account, _)| main_account);
        }
        if account_name == primitives::account_derive::RESERVED_NAME_FEE {
            return test_institution_by_cid(cid_number).map(|(_, _, fee_account)| fee_account);
        }
        None
    }

    fn lookup_cid(addr: &AccountId32) -> Option<Vec<u8>> {
        test_institution(addr).map(|(cid_number, _, _)| cid_number)
    }

    fn lookup_org(addr: &AccountId32) -> Option<InstitutionCode> {
        test_institution(addr).map(|(_, institution_code, _)| institution_code)
    }

    fn lookup_admin_config(
        addr: &AccountId32,
    ) -> Option<primitives::multisig::MultisigConfigSnapshot<AccountId32>> {
        let (_, _, admins) = test_institution(addr)?;
        let admins_len = u32::try_from(admins.len()).ok()?;
        Some(primitives::multisig::MultisigConfigSnapshot {
            admins,
            admins_len,
            threshold: admins_len,
        })
    }

    fn account_exists(addr: &AccountId32) -> bool {
        test_institution(addr).is_some()
    }
}

fn test_institution_by_cid(
    cid_number: &[u8],
) -> Option<(InstitutionCode, AccountId32, AccountId32)> {
    CHINA_CB
        .iter()
        .find_map(|institution| {
            (institution.cid_number.as_bytes() == cid_number).then(|| {
                (
                    votingengine::types::institution_code_from_cid_number(institution.cid_number)
                        .expect("储委会 CID 必须包含有效机构码"),
                    AccountId32::new(institution.main_account),
                    AccountId32::new(institution.fee_account),
                )
            })
        })
        .or_else(|| {
            CHINA_CH.iter().find_map(|institution| {
                (institution.cid_number.as_bytes() == cid_number).then(|| {
                    (
                        votingengine::types::institution_code_from_cid_number(
                            institution.cid_number,
                        )
                        .expect("省储行 CID 必须包含有效机构码"),
                        AccountId32::new(institution.main_account),
                        AccountId32::new(institution.fee_account),
                    )
                })
            })
        })
}

fn test_institution(addr: &AccountId32) -> Option<(Vec<u8>, InstitutionCode, Vec<AccountId32>)> {
    CHINA_CB
        .iter()
        .filter_map(|institution| {
            let institution_code =
                votingengine::types::institution_code_from_cid_number(institution.cid_number)?;
            matches!(institution_code, NRC | PRC)
                .then(|| {
                    (
                        institution.cid_number.as_bytes().to_vec(),
                        institution_code,
                        institution
                            .admins
                            .iter()
                            .copied()
                            .map(AccountId32::new)
                            .collect(),
                    )
                })
                .filter(|_| AccountId32::new(institution.main_account) == *addr)
        })
        .next()
        .or_else(|| {
            CHINA_CH.iter().find_map(|institution| {
                let institution_code =
                    votingengine::types::institution_code_from_cid_number(institution.cid_number)?;
                (institution_code == PRB && AccountId32::new(institution.main_account) == *addr)
                    .then(|| {
                        (
                            institution.cid_number.as_bytes().to_vec(),
                            institution_code,
                            institution
                                .admins
                                .iter()
                                .copied()
                                .map(AccountId32::new)
                                .collect(),
                        )
                    })
            })
        })
}

fn nrc_admin(index: usize) -> AccountId32 {
    AccountId32::new(CHINA_CB[0].admins[index])
}

fn prc_admin(index: usize) -> AccountId32 {
    AccountId32::new(CHINA_CB[1].admins[index])
}

fn prb_admin(index: usize) -> AccountId32 {
    AccountId32::new(CHINA_CH[0].admins[index])
}

fn nrc_cid() -> CidNumber {
    CHINA_CB[0]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("NRC CID fits runtime bound")
}

fn prc_cid() -> CidNumber {
    CHINA_CB[1]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("PRC CID fits runtime bound")
}

fn prb_cid() -> CidNumber {
    CHINA_CH[0]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("PRB CID fits runtime bound")
}

fn nrc_pallet_id() -> AccountId32 {
    AccountId32::new(CHINA_CB[0].main_account)
}

fn prc_pallet_id() -> AccountId32 {
    AccountId32::new(CHINA_CB[1].main_account)
}

fn prb_pallet_id() -> AccountId32 {
    AccountId32::new(CHINA_CH[0].main_account)
}

fn institution_account_id(institution: &AccountId32) -> AccountId32 {
    institution.clone()
}

/// 获取最近一次 create_internal_proposal 分配的 proposal_id。
fn last_proposal_id() -> u64 {
    votingengine::Pallet::<Test>::next_proposal_id().saturating_sub(1)
}

/// 测试辅助:走投票引擎公开 `internal_vote` extrinsic 投票(统一入口)。
fn cast_vote(who: AccountId32, proposal_id: u64, approve: bool) -> DispatchResult {
    let proposal = VotingEngine::proposals(proposal_id)
        .ok_or(votingengine::Error::<Test>::ProposalNotFound)?;
    let actor_cid_number = proposal
        .actor_cid_number
        .ok_or(votingengine::Error::<Test>::InvalidInstitution)?;
    let institution_code = core::str::from_utf8(actor_cid_number.as_slice())
        .ok()
        .and_then(votingengine::types::institution_code_from_cid_number)
        .ok_or(votingengine::Error::<Test>::InvalidInstitution)?;
    let ticket_claim =
        internalvote::InternalVoteTicketClaim::InstitutionRole(bounded_test_role(institution_code));
    let result = frame_support::storage::with_transaction(
        || -> frame_support::storage::TransactionOutcome<DispatchResult> {
            match internalvote::Pallet::<Test>::do_internal_vote(
                who,
                proposal_id,
                ticket_claim,
                approve,
            ) {
                Ok(()) => frame_support::storage::TransactionOutcome::Commit(Ok(())),
                Err(e) => frame_support::storage::TransactionOutcome::Rollback(Err(e)),
            }
        },
    );
    if result.is_ok()
        && VotingEngine::proposals(proposal_id)
            .map(|proposal| proposal.status != votingengine::STATUS_VOTING)
            .unwrap_or(false)
    {
        <VotingEngine as Hooks<u64>>::on_initialize(System::block_number());
    }
    result
}

fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("test storage should build");

    let balances = vec![
        (institution_account_id(&nrc_pallet_id()), 1_000),
        (institution_account_id(&prc_pallet_id()), 1_000),
        (institution_account_id(&prb_pallet_id()), 1_000),
        (AccountId32::new(CHINA_CB[0].fee_account), 1_000),
        (AccountId32::new(CHINA_CB[1].fee_account), 1_000),
        (AccountId32::new(CHINA_CH[0].fee_account), 1_000),
    ];
    pallet_balances::GenesisConfig::<Test> {
        balances,
        ..Default::default()
    }
    .assimilate_storage(&mut storage)
    .expect("balances should assimilate");

    storage.into()
}

mod cases {
    use super::*;

    #[test]
    fn nrc_destroy_executes_when_yes_votes_reach_threshold() {
        new_test_ext().execute_with(|| {
            let institution = nrc_pallet_id();
            let account = institution_account_id(&institution);

            assert_ok!(ResolutionDestroy::propose_destroy(
                RuntimeOrigin::signed(nrc_admin(0)),
                nrc_cid(),
                bounded_test_role(NRC),
                institution.clone(),
                100
            ));
            let pid = last_proposal_id();

            for i in 1..13 {
                assert_ok!(cast_vote(nrc_admin(i), pid, true));
            }

            assert_eq!(Balances::free_balance(&account), 900);
            assert_eq!(
                Balances::free_balance(AccountId32::new(CHINA_CB[0].fee_account)),
                990
            );
        });
    }

    #[test]
    fn destroy_fee_shortage_never_falls_back_to_admin() {
        new_test_ext().execute_with(|| {
            let institution = nrc_pallet_id();
            let account = institution_account_id(&institution);
            let fee_account = AccountId32::new(CHINA_CB[0].fee_account);
            let admin = nrc_admin(0);
            assert_ok!(Balances::force_set_balance(
                RuntimeOrigin::root(),
                fee_account.clone(),
                1,
            ));
            assert_ok!(Balances::force_set_balance(
                RuntimeOrigin::root(),
                admin.clone(),
                10_000,
            ));

            assert_ok!(ResolutionDestroy::propose_destroy(
                RuntimeOrigin::signed(admin.clone()),
                nrc_cid(),
                bounded_test_role(NRC),
                institution,
                100,
            ));
            let pid = last_proposal_id();
            for i in 1..13 {
                assert_ok!(cast_vote(nrc_admin(i), pid, true));
            }

            assert_eq!(Balances::free_balance(&account), 1_000);
            assert_eq!(Balances::free_balance(&fee_account), 1);
            assert_eq!(Balances::free_balance(&admin), 10_000);
            assert_eq!(
                votingengine::Pallet::<Test>::proposals(pid)
                    .expect("proposal should exist")
                    .status,
                STATUS_PASSED
            );
        });
    }

    #[test]
    fn prc_destroy_executes_when_yes_votes_reach_threshold() {
        new_test_ext().execute_with(|| {
            let institution = prc_pallet_id();
            let account = institution_account_id(&institution);

            assert_ok!(ResolutionDestroy::propose_destroy(
                RuntimeOrigin::signed(prc_admin(0)),
                prc_cid(),
                bounded_test_role(PRC),
                institution.clone(),
                200
            ));
            let pid = last_proposal_id();

            for i in 1..6 {
                assert_ok!(cast_vote(prc_admin(i), pid, true));
            }

            assert_eq!(Balances::free_balance(&account), 800);
        });
    }

    #[test]
    fn prb_destroy_executes_when_yes_votes_reach_threshold() {
        new_test_ext().execute_with(|| {
            let institution = prb_pallet_id();
            let account = institution_account_id(&institution);

            assert_ok!(ResolutionDestroy::propose_destroy(
                RuntimeOrigin::signed(prb_admin(0)),
                prb_cid(),
                bounded_test_role(PRB),
                institution.clone(),
                300
            ));
            let pid = last_proposal_id();

            for i in 1..6 {
                assert_ok!(cast_vote(prb_admin(i), pid, true));
            }

            assert_eq!(Balances::free_balance(&account), 700);
        });
    }

    #[test]
    fn destroy_business_rejects_other_internal_vote_institutions() {
        new_test_ext().execute_with(|| {
            let njd_code = votingengine::types::NJD;
            let njd = primitives::cid::china::china_sf::CHINA_SF
                .iter()
                .find(|node| {
                    votingengine::types::institution_code_from_cid_number(node.cid_number)
                        == Some(njd_code)
                })
                .map(|node| {
                    (
                        CidNumber::try_from(node.cid_number.as_bytes().to_vec())
                            .expect("NJD CID fits runtime bound"),
                        AccountId32::new(node.main_account),
                    )
                })
                .expect("NJD must exist in CHINA_SF");

            assert_noop!(
                ResolutionDestroy::propose_destroy(
                    RuntimeOrigin::signed(nrc_admin(0)),
                    njd.0,
                    bounded_test_role(njd_code),
                    njd.1,
                    100,
                ),
                Error::<Test>::InvalidInstitution
            );
        });
    }

    #[test]
    fn non_admin_cannot_propose_or_vote() {
        new_test_ext().execute_with(|| {
            let institution = nrc_pallet_id();

            assert_noop!(
                ResolutionDestroy::propose_destroy(
                    RuntimeOrigin::signed(prc_admin(0)),
                    nrc_cid(),
                    bounded_test_role(NRC),
                    institution.clone(),
                    100
                ),
                Error::<Test>::UnauthorizedAdmin
            );

            assert_ok!(ResolutionDestroy::propose_destroy(
                RuntimeOrigin::signed(nrc_admin(0)),
                nrc_cid(),
                bounded_test_role(NRC),
                institution.clone(),
                100
            ));
            let pid = last_proposal_id();

            assert_noop!(
                cast_vote(prc_admin(0), pid, true),
                votingengine::pallet::Error::<Test>::NoPermission
            );
        });
    }

    #[test]
    fn zero_amount_and_insufficient_balance_are_rejected() {
        new_test_ext().execute_with(|| {
            let institution = nrc_pallet_id();

            assert_noop!(
                ResolutionDestroy::propose_destroy(
                    RuntimeOrigin::signed(nrc_admin(0)),
                    nrc_cid(),
                    bounded_test_role(NRC),
                    institution.clone(),
                    0
                ),
                Error::<Test>::ZeroAmount
            );

            assert_ok!(ResolutionDestroy::propose_destroy(
                RuntimeOrigin::signed(nrc_admin(0)),
                nrc_cid(),
                bounded_test_role(NRC),
                institution.clone(),
                2_000
            ));
            let pid = last_proposal_id();

            for i in 1..12 {
                assert_ok!(cast_vote(nrc_admin(i), pid, true));
            }

            // 发起人已自动赞成，最后一张补票触发自动执行失败路径。
            assert_ok!(cast_vote(nrc_admin(12), pid, true));
            assert_eq!(
                votingengine::Pallet::<Test>::proposals(pid)
                    .expect("proposal should exist")
                    .status,
                STATUS_PASSED
            );
            assert_eq!(
                Balances::free_balance(institution_account_id(&institution)),
                1_000
            );
            assert!(votingengine::Pallet::<Test>::get_proposal_data(pid).is_some());
            assert_ok!(VotingEngine::retry_passed_proposal(
                RuntimeOrigin::signed(nrc_admin(0)),
                pid
            ));
            assert_eq!(
                votingengine::Pallet::<Test>::proposal_execution_retry_state(pid)
                    .expect("retry state should exist")
                    .manual_attempts,
                1
            );
            assert_eq!(
                votingengine::Pallet::<Test>::proposals(pid)
                    .expect("proposal should exist")
                    .status,
                STATUS_PASSED
            );
        });
    }

    #[test]
    fn existential_deposit_is_preserved() {
        new_test_ext().execute_with(|| {
            let institution = nrc_pallet_id();
            let account = institution_account_id(&institution);

            assert_ok!(ResolutionDestroy::propose_destroy(
                RuntimeOrigin::signed(nrc_admin(0)),
                nrc_cid(),
                bounded_test_role(NRC),
                institution.clone(),
                1_000
            ));
            let pid = last_proposal_id();

            for i in 1..13 {
                assert_ok!(cast_vote(nrc_admin(i), pid, true));
            }

            // 如果不校验 ED，这里会被销毁到 0 并触发账户 reap。
            assert_eq!(Balances::free_balance(&account), 1_000);
            assert_ok!(VotingEngine::retry_passed_proposal(
                RuntimeOrigin::signed(nrc_admin(0)),
                pid
            ));
            assert_eq!(Balances::free_balance(&account), 1_000);
            assert_eq!(
                votingengine::Pallet::<Test>::proposal_execution_retry_state(pid)
                    .expect("retry state should exist")
                    .manual_attempts,
                1
            );
        });
    }

    #[test]
    fn rejected_proposal_does_not_block_new_proposal() {
        new_test_ext().execute_with(|| {
            let institution = nrc_pallet_id();
            assert_ok!(ResolutionDestroy::propose_destroy(
                RuntimeOrigin::signed(nrc_admin(0)),
                nrc_cid(),
                bounded_test_role(NRC),
                institution.clone(),
                100
            ));
            let pid1 = last_proposal_id();

            let end = votingengine::Pallet::<Test>::proposals(pid1)
                .expect("proposal should exist")
                .end;
            System::set_block_number(end + 1);
            assert_ok!(votingengine::Pallet::<Test>::finalize_proposal(
                RuntimeOrigin::signed(nrc_admin(0)),
                pid1
            ));
            assert_eq!(
                votingengine::Pallet::<Test>::proposals(pid1)
                    .expect("proposal should exist")
                    .status,
                STATUS_REJECTED
            );

            assert_ok!(ResolutionDestroy::propose_destroy(
                RuntimeOrigin::signed(nrc_admin(0)),
                nrc_cid(),
                bounded_test_role(NRC),
                institution.clone(),
                50
            ));
            let pid2 = last_proposal_id();
            // 提案 2 应该已创建
            assert!(votingengine::Pallet::<Test>::get_proposal_data(pid2).is_some());
        });
    }

    #[test]
    fn execute_destroy_succeeds_after_failed_auto_execution() {
        new_test_ext().execute_with(|| {
            let institution = nrc_pallet_id();
            let account = institution_account_id(&institution);

            assert_ok!(ResolutionDestroy::propose_destroy(
                RuntimeOrigin::signed(nrc_admin(0)),
                nrc_cid(),
                bounded_test_role(NRC),
                institution.clone(),
                1_100
            ));
            let pid = last_proposal_id();

            for i in 1..13 {
                assert_ok!(cast_vote(nrc_admin(i), pid, true));
            }

            // 自动执行失败后状态保留为 PASSED，补充余额后可手动重试。
            assert_eq!(
                votingengine::Pallet::<Test>::proposals(pid)
                    .expect("proposal should exist")
                    .status,
                STATUS_PASSED
            );
            assert_eq!(Balances::free_balance(&account), 1_000);
            assert!(votingengine::Pallet::<Test>::get_proposal_data(pid).is_some());

            // 补充余额后手动重试执行
            let _ = Balances::deposit_creating(&account, 200);
            assert_ok!(VotingEngine::retry_passed_proposal(
                RuntimeOrigin::signed(nrc_admin(0)),
                pid
            ));
            assert_eq!(Balances::free_balance(&account), 100);
        });
    }

    #[test]
    fn executed_proposal_does_not_block_new_proposal() {
        new_test_ext().execute_with(|| {
            let institution = nrc_pallet_id();
            assert_ok!(ResolutionDestroy::propose_destroy(
                RuntimeOrigin::signed(nrc_admin(0)),
                nrc_cid(),
                bounded_test_role(NRC),
                institution.clone(),
                100
            ));
            let pid1 = last_proposal_id();

            for i in 1..13 {
                assert_ok!(cast_vote(nrc_admin(i), pid1, true));
            }

            assert_ok!(ResolutionDestroy::propose_destroy(
                RuntimeOrigin::signed(nrc_admin(0)),
                nrc_cid(),
                bounded_test_role(NRC),
                institution.clone(),
                50
            ));
            let pid2 = last_proposal_id();
            assert!(votingengine::Pallet::<Test>::get_proposal_data(pid2).is_some());
        });
    }

    #[test]
    fn duplicate_vote_is_rejected_by_votingengine() {
        new_test_ext().execute_with(|| {
            let institution = nrc_pallet_id();
            assert_ok!(ResolutionDestroy::propose_destroy(
                RuntimeOrigin::signed(nrc_admin(0)),
                nrc_cid(),
                bounded_test_role(NRC),
                institution.clone(),
                100
            ));
            let pid = last_proposal_id();
            assert_ok!(cast_vote(nrc_admin(1), pid, true));
            assert_noop!(
                cast_vote(nrc_admin(1), pid, true),
                votingengine::pallet::Error::<Test>::AlreadyVoted
            );
        });
    }

    #[test]
    fn execute_destroy_requires_role_voter_snapshot() {
        new_test_ext().execute_with(|| {
            let institution = nrc_pallet_id();
            let account = institution_account_id(&institution);
            let outsider = AccountId32::new([99u8; 32]);

            assert_ok!(ResolutionDestroy::propose_destroy(
                RuntimeOrigin::signed(nrc_admin(0)),
                nrc_cid(),
                bounded_test_role(NRC),
                institution.clone(),
                1_100
            ));
            let pid = last_proposal_id();
            for i in 1..13 {
                assert_ok!(cast_vote(nrc_admin(i), pid, true));
            }
            let _ = Balances::deposit_creating(&account, 200);
            assert_noop!(
                VotingEngine::retry_passed_proposal(RuntimeOrigin::signed(outsider), pid),
                votingengine::pallet::Error::<Test>::NoPermission
            );
            assert_ok!(VotingEngine::retry_passed_proposal(
                RuntimeOrigin::signed(nrc_admin(0)),
                pid
            ));
            assert_eq!(Balances::free_balance(&account), 100);
        });
    }

    #[test]
    fn institution_query_returns_none_for_invalid_account() {
        new_test_ext().execute_with(|| {
            assert_eq!(
            <TestInstitutionQuery as entitybase::InstitutionMultisigQuery<AccountId32>>::lookup_org(
                &AccountId32::new([0u8; 32])
            ),
            None
        );
        });
    }
}

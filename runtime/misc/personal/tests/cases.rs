//! 验证个人多签创建、关闭及投票执行和拒绝行为。

use super::*;
use codec::{Decode, Encode};
use engingcore::{STATUS_EXECUTED, STATUS_EXECUTION_FAILED, STATUS_REJECTED, STATUS_VOTING};
use frame_support::{
    assert_noop, assert_ok,
    traits::{Currency, Hooks},
    BoundedVec,
};
use sp_runtime::DispatchError;

const CREATE_AMOUNT: Balance = 1_000;
const CREATE_FEE: Balance = 10; // calculate_onchain_fee(1000) = max(1000*0.001, 10) = 10
const SEED_BALANCE: Balance = 5_000;

fn process_pending_execution() {
    <VotingEngine as Hooks<u64>>::on_initialize(System::block_number());
}

fn setup_creator_balance() -> AccountId32 {
    let c = creator_account_id();
    let _ = Balances::deposit_creating(&c, SEED_BALANCE);
    c
}

fn proposed_account_id(creator_account_id: &AccountId32, name: &[u8]) -> AccountId32 {
    Personal::derive_personal_account(creator_account_id, name).expect("derive should succeed")
}

fn create_rejected_event_count(pid: u128) -> usize {
    System::events()
        .iter()
        .filter(|record| {
            matches!(
                &record.event,
                RuntimeEvent::Personal(pallet::Event::PersonalCreateRejected {
                    proposal_id,
                    ..
                }) if *proposal_id == pid
            )
        })
        .count()
}

fn create_failed_event_count(pid: u128) -> usize {
    System::events()
        .iter()
        .filter(|record| {
            matches!(
                &record.event,
                RuntimeEvent::Personal(pallet::Event::CreateExecutionFailed {
                    proposal_id,
                    ..
                }) if *proposal_id == pid
            )
        })
        .count()
}

fn close_failed_event_count(pid: u128) -> usize {
    System::events()
        .iter()
        .filter(|record| {
            matches!(
                &record.event,
                RuntimeEvent::Personal(pallet::Event::CloseExecutionFailed {
                    proposal_id,
                    ..
                }) if *proposal_id == pid
            )
        })
        .count()
}

fn overwrite_create_proposal_fee(pid: u128, fee: Balance) {
    let mut action = pallet::PendingPersonalCreate::<Test>::get(pid).expect("pending action");
    action.fee = fee;
    let mut data = alloc::vec::Vec::from(crate::MODULE_TAG);
    data.push(crate::ACTION_CREATE);
    data.extend_from_slice(&action.encode());
    let bounded: BoundedVec<u8, <Test as engingcore::Config>::MaxProposalDataLen> =
        BoundedVec::try_from(data).expect("proposal data fits");
    engingcore::ProposalData::<Test>::insert(pid, bounded);
}

// ─── 1. propose_create:写 Pending + reserve fee + 发事件 ─────────────

/// 个人成员没有CID字段；空备注姓名由normalize_names填默认，授权仍只认钱包。
#[test]
fn propose_create_uses_wallet_members_with_optional_name_notes() {
    new_test_ext().execute_with(|| {
        let c = setup_creator_balance();
        // 姓、名备注为空，成员仍由钱包账户识别。
        let admins: pallet::AdminsOf<Test> = (0..3u8)
            .map(admin)
            .map(|account_id| crate::types::Admin {
                account_id,

                family_name: Default::default(),
                given_name: Default::default(),
            })
            .collect::<Vec<_>>()
            .try_into()
            .expect("admins fit");
        let name = account_name(b"no-cid-personal");
        let dq = proposed_account_id(&c, b"no-cid-personal");

        // 空姓名备注不影响创建。
        assert_ok!(Personal::propose_create(
            RuntimeOrigin::signed(c.clone()),
            name,
            admins,
            2,
            CREATE_AMOUNT,
        ));
        assert!(pallet::PersonalAccounts::<Test>::contains_key(&dq));
    });
}

#[test]
fn propose_create_writes_pending_and_reserves_fee() {
    new_test_ext().execute_with(|| {
        let c = setup_creator_balance();
        let admins = admins_vec(3); // admin(0)..admin(2)
        let name = account_name(b"alice-personal");
        let dq = proposed_account_id(&c, b"alice-personal");

        assert_ok!(Personal::propose_create(
            RuntimeOrigin::signed(c.clone()),
            name.clone(),
            admins.clone(),
            2,
            CREATE_AMOUNT,
        ));

        let pid = last_proposal_id();

        assert!(pallet::PendingPersonalCreate::<Test>::contains_key(pid));
        assert!(pallet::PersonalAccounts::<Test>::contains_key(&dq));
        let pending_action = pallet::PendingPersonalCreate::<Test>::get(pid).unwrap();
        assert_eq!(pending_action.fee, CREATE_FEE);
        let pending_account = pallet::PersonalAccounts::<Test>::get(&dq).unwrap();
        assert_eq!(pending_account.status, types::PersonalStatus::Pending);
        assert_eq!(pending_account.account_name, name);
        assert_eq!(
            internalvote::PendingPersonalThresholds::<Test>::get(pid),
            Some(2)
        );
        assert_eq!(
            internalvote::InternalThresholdSnapshot::<Test>::get(pid),
            Some(3)
        );
        // creator_account_id 已被 reserve(amount + fee)
        assert_eq!(Balances::reserved_balance(&c), CREATE_AMOUNT + CREATE_FEE);
        assert_eq!(
            Balances::free_balance(&c),
            SEED_BALANCE - CREATE_AMOUNT - CREATE_FEE
        );

        // 投票引擎里提案在投票中
        let proposal = engingcore::Pallet::<Test>::proposals(pid).expect("proposal exists");
        assert_eq!(proposal.status, STATUS_VOTING);
    });
}

// ─── 2. 投票通过 → 创建账户 → release reserve ──────────────────────────

#[test]
fn create_executes_when_internal_vote_reaches_threshold() {
    new_test_ext().execute_with(|| {
        let c = setup_creator_balance();
        let admins = admins_vec(3);
        let admin_accounts: alloc::vec::Vec<AccountId32> = (0..3u8).map(admin).collect();
        let name = account_name(b"alice-personal");
        let dq = proposed_account_id(&c, b"alice-personal");

        assert_ok!(Personal::propose_create(
            RuntimeOrigin::signed(c.clone()),
            name,
            admins,
            2,
            CREATE_AMOUNT,
        ));
        let pid = last_proposal_id();

        // 创建提案要求"全员通过"——投票引擎 threshold = admins.len() = 3
        assert_ok!(cast_yes_votes(&admin_accounts[1..], 2, pid));
        process_pending_execution();

        // 提案进入 EXECUTED
        let proposal = engingcore::Pallet::<Test>::proposals(pid).expect("proposal exists");
        assert_eq!(proposal.status, STATUS_EXECUTED);

        // 多签账户激活,资金到位,Pending 已清
        let dq_state = pallet::PersonalAccounts::<Test>::get(&dq).expect("active multisig");
        assert_eq!(dq_state.status, types::PersonalStatus::Active);
        let account_id = crate::AdminAccounts::<Test>::get(dq.clone())
            .expect("personal admins should be active");
        assert_eq!(account_id.status, crate::AdminAccountStatus::Active);

        let account_id = dq.clone();
        assert_eq!(
            internalvote::ActivePersonalThresholds::<Test>::get(account_id),
            Some(2)
        );
        assert_eq!(Balances::free_balance(&dq), CREATE_AMOUNT);
        assert_eq!(Balances::reserved_balance(&c), 0);
        assert!(!pallet::PendingPersonalCreate::<Test>::contains_key(pid));
    });
}

// ─── 3. 投票拒绝 → 引擎回调释放 reserve + 发拒绝事件 ─────────────────

#[test]
fn create_rejected_cleanup_releases_reserve_and_emits_event() {
    new_test_ext().execute_with(|| {
        let c = setup_creator_balance();
        let admins = admins_vec(3);
        let admin_accounts: alloc::vec::Vec<AccountId32> = (0..3u8).map(admin).collect();
        let name = account_name(b"alice-personal");
        let dq = proposed_account_id(&c, b"alice-personal");

        assert_ok!(Personal::propose_create(
            RuntimeOrigin::signed(c.clone()),
            name,
            admins,
            2,
            CREATE_AMOUNT,
        ));
        let pid = last_proposal_id();

        // 一票否决:全员通过制度下,只要有人反对就立刻进 STATUS_REJECTED
        assert_ok!(cast_no_votes(&admin_accounts[1..], 1, pid));

        let proposal = engingcore::Pallet::<Test>::proposals(pid).expect("proposal exists");
        assert_eq!(proposal.status, STATUS_REJECTED);

        // 拒绝路径下 Executor 应已 cleanup,reserve 释放,storage 清空
        assert_eq!(Balances::reserved_balance(&c), 0);
        assert!(!pallet::PersonalAccounts::<Test>::contains_key(&dq));
        assert!(!pallet::PendingPersonalCreate::<Test>::contains_key(pid));
        assert_eq!(create_rejected_event_count(pid), 1);
    });
}

// ─── 4. 重复账户被拒绝 ────────────────────────────────────────────────

#[test]
fn propose_create_rejects_duplicate_personal_account() {
    new_test_ext().execute_with(|| {
        let c = setup_creator_balance();
        let dq = proposed_account_id(&c, b"alice-personal");
        // 直接把目标地址灌成 Active,模拟"地址已存在"
        seed_active_multisig(&dq, &c, &[admin(0), admin(1), admin(2)], 500);

        assert_noop!(
            Personal::propose_create(
                RuntimeOrigin::signed(c),
                account_name(b"alice-personal"),
                admins_vec(3),
                2,
                CREATE_AMOUNT,
            ),
            pallet::Error::<Test>::PersonalAlreadyExists
        );
    });
}

// ─── 5. 普通业务阈值由用户传入，投票引擎统一校验保存 ───────────────────

#[test]
fn propose_create_stores_regular_threshold_and_uses_all_admin_create_threshold() {
    new_test_ext().execute_with(|| {
        let c = setup_creator_balance();
        assert_ok!(Personal::propose_create(
            RuntimeOrigin::signed(c.clone()),
            account_name(b"derived-threshold"),
            admins_vec(3),
            2,
            CREATE_AMOUNT,
        ));
        let pid = last_proposal_id();
        assert_eq!(
            internalvote::PendingPersonalThresholds::<Test>::get(pid),
            Some(2)
        );
        assert_eq!(
            internalvote::InternalThresholdSnapshot::<Test>::get(pid),
            Some(3)
        );
    });
}

#[test]
fn two_admin_personal_create_uses_two_of_two_for_regular_and_create_threshold() {
    new_test_ext().execute_with(|| {
        let c = setup_creator_balance();
        assert_ok!(Personal::propose_create(
            RuntimeOrigin::signed(c),
            account_name(b"two-admin"),
            admins_vec(2),
            2,
            CREATE_AMOUNT,
        ));
        let pid = last_proposal_id();
        assert_eq!(
            internalvote::PendingPersonalThresholds::<Test>::get(pid),
            Some(2)
        );
        assert_eq!(
            internalvote::InternalThresholdSnapshot::<Test>::get(pid),
            Some(2)
        );
    });
}

#[test]
fn sixty_four_admin_personal_create_is_allowed_and_uses_full_create_threshold() {
    new_test_ext().execute_with(|| {
        let institution_limit = <<Test as engingcore::Config>::MaxSnapshotVoters as frame_support::traits::Get<u32>>::get();
        let personal_limit = <<Test as Config>::MaxPersonalAccountAdmins as frame_support::traits::Get<u32>>::get();
        assert_eq!(institution_limit, 1);
        assert_eq!(personal_limit, 64);
        let c = setup_creator_balance();
        assert_ok!(Personal::propose_create(
            RuntimeOrigin::signed(c),
            account_name(b"sixty-four-admins"),
            admins_vec(64),
            33,
            CREATE_AMOUNT,
        ));
        let pid = last_proposal_id();
        assert_eq!(
            internalvote::PendingPersonalThresholds::<Test>::get(pid),
            Some(33)
        );
        assert_eq!(
            internalvote::InternalThresholdSnapshot::<Test>::get(pid),
            Some(64)
        );
        assert_eq!(
            engingcore::Pallet::<Test>::snapshot_admins_len(
                pid,
                engingcore::ProposalSubject::PersonalAccount(proposed_account_id(
                    &creator_account_id(),
                    b"sixty-four-admins"
                ))
            ),
            Some(64)
        );
    });
}

#[test]
fn sixty_five_admin_personal_create_cannot_be_encoded() {
    new_test_ext().execute_with(|| {
        let admins: alloc::vec::Vec<crate::types::Admin<AccountId32>> =
            (0..65u8).map(admin).map(admin_record).collect();
        assert!(pallet::AdminsOf::<Test>::try_from(admins).is_err());
    });
}

// ─── 6. admin 重复 ────────────────────────────────────────────────────

#[test]
fn propose_create_rejects_duplicate_admins() {
    new_test_ext().execute_with(|| {
        let c = setup_creator_balance();
        let dup_admins = admin_records(vec![admin(0), admin(1), admin(0)]); // admin(0) 重复

        assert_noop!(
            Personal::propose_create(
                RuntimeOrigin::signed(c),
                account_name(b"dup"),
                dup_admins,
                2,
                CREATE_AMOUNT,
            ),
            pallet::Error::<Test>::DuplicateAdmin
        );
    });
}

// ─── 7. 入金低于最小门槛 ───────────────────────────────────────────────

#[test]
fn propose_create_rejects_below_minimum_amount() {
    new_test_ext().execute_with(|| {
        let c = setup_creator_balance();
        // MinCreateAmount = 111
        assert_noop!(
            Personal::propose_create(
                RuntimeOrigin::signed(c),
                account_name(b"too-small"),
                admins_vec(3),
                2,
                100, // 100 < 111
            ),
            pallet::Error::<Test>::CreateAmountBelowMinimum
        );
    });
}

#[test]
fn propose_create_rejects_reserved_and_protected_accounts() {
    new_test_ext().execute_with(|| {
        let c = setup_creator_balance();
        let protected = proposed_account_id(&c, b"protected-target");
        set_protected_account(Some(protected));

        assert_noop!(
            Personal::propose_create(
                RuntimeOrigin::signed(c.clone()),
                account_name(b"protected-target"),
                admins_vec(3),
                2,
                CREATE_AMOUNT,
            ),
            pallet::Error::<Test>::ProtectedSource
        );

        set_protected_account(Some(c.clone()));
        assert_noop!(
            Personal::propose_create(
                RuntimeOrigin::signed(c),
                account_name(b"protected-creator_account_id"),
                admins_vec(3),
                2,
                CREATE_AMOUNT,
            ),
            pallet::Error::<Test>::ProtectedSource
        );
    });
}

// ─── 8. propose_close 写 Pending + 阻止并发 ───────────────────────────

#[test]
fn propose_close_writes_pending_and_blocks_concurrent() {
    new_test_ext().execute_with(|| {
        let c = setup_creator_balance();
        let dq = proposed_account_id(&c, b"close-pending");
        let admins_acc = vec![admin(0), admin(1), admin(2)];
        seed_active_multisig(&dq, &c, &admins_acc, 1_000);

        let beneficiary_acc = beneficiary_account_id();

        assert_ok!(Personal::propose_close(
            RuntimeOrigin::signed(admin(0)),
            dq.clone(),
            beneficiary_acc.clone(),
        ));

        let pid = last_proposal_id();
        assert_eq!(pallet::PendingCloseProposal::<Test>::get(&dq), Some(pid));

        // 第二次发起应被阻止
        assert_noop!(
            Personal::propose_close(RuntimeOrigin::signed(admin(1)), dq, beneficiary_acc,),
            pallet::Error::<Test>::CloseAlreadyPending
        );
    });
}

// ─── 9. 投票通过关闭 → 余额转出 → 删 storage ──────────────────────────

#[test]
fn close_executes_when_internal_vote_reaches_threshold() {
    new_test_ext().execute_with(|| {
        let c = setup_creator_balance();
        let dq = proposed_account_id(&c, b"close-active");
        let admins_acc = vec![admin(0), admin(1), admin(2)];
        seed_active_multisig(&dq, &c, &admins_acc, 1_000);
        let beneficiary_acc = beneficiary_account_id();

        assert_ok!(Personal::propose_close(
            RuntimeOrigin::signed(admin(0)),
            dq.clone(),
            beneficiary_acc.clone(),
        ));
        let pid = last_proposal_id();

        // 关闭提案要求全员通过(3 票)
        assert_ok!(cast_yes_votes(&admins_acc[1..], 2, pid));
        process_pending_execution();

        let proposal = engingcore::Pallet::<Test>::proposals(pid).expect("proposal exists");
        assert_eq!(proposal.status, STATUS_EXECUTED);

        // amount 1000 → fee = max(1, 10) = 10,beneficiary_account_id 收 990
        assert_eq!(Balances::free_balance(&beneficiary_acc), 990);
        assert_eq!(Balances::free_balance(&dq), 0);
        let account_id = dq.clone();
        assert!(!pallet::PersonalAccounts::<Test>::contains_key(&dq));
        assert!(!pallet::PendingCloseProposal::<Test>::contains_key(&dq));
        assert!(crate::AdminAccounts::<Test>::get(account_id.clone()).is_none());
        assert!(internalvote::ActivePersonalThresholds::<Test>::get(account_id).is_none());

        assert_ok!(Personal::propose_create(
            RuntimeOrigin::signed(c),
            account_name(b"close-active"),
            admins_vec(3),
            2,
            CREATE_AMOUNT,
        ));
    });
}

// ─── 10. 关闭余额过低被拒(链安全保护) ──────────────────────────────────

#[test]
fn propose_close_rejects_when_balance_below_minimum() {
    new_test_ext().execute_with(|| {
        let c = setup_creator_balance();
        let dq = proposed_account_id(&c, b"low-balance");
        let admins_acc = vec![admin(0), admin(1), admin(2)];
        // 余额 10 分只够最低执行费，扣费后无法向受益人转出 ED，应拒绝。
        seed_active_multisig(&dq, &c, &admins_acc, 10);

        assert_noop!(
            Personal::propose_close(
                RuntimeOrigin::signed(admin(0)),
                dq,
                beneficiary_account_id(),
            ),
            pallet::Error::<Test>::CloseBalanceBelowMinimum
        );
    });
}

#[test]
fn propose_close_rejects_reserved_and_protected_beneficiary() {
    new_test_ext().execute_with(|| {
        let c = setup_creator_balance();
        let dq = proposed_account_id(&c, b"close-protected");
        let admins_acc = vec![admin(0), admin(1), admin(2)];
        seed_active_multisig(&dq, &c, &admins_acc, 1_000);

        assert_noop!(
            Personal::propose_close(
                RuntimeOrigin::signed(admin(0)),
                dq.clone(),
                AccountId32::new([0xAA; 32]),
            ),
            pallet::Error::<Test>::InvalidBeneficiary
        );

        let protected = beneficiary_account_id();
        set_protected_account(Some(protected.clone()));
        assert_noop!(
            Personal::propose_close(RuntimeOrigin::signed(admin(0)), dq, protected,),
            pallet::Error::<Test>::InvalidBeneficiary
        );
    });
}

// ─── 11. 创建执行失败 → 终态清理 reserve + pending + 失败事件 ───────────

#[test]
fn create_execution_failed_terminal_cleans_pending_and_emits_once() {
    new_test_ext().execute_with(|| {
        let c = setup_creator_balance();
        let admins = admins_vec(3);
        let admin_accounts: alloc::vec::Vec<AccountId32> = (0..3u8).map(admin).collect();
        let dq = proposed_account_id(&c, b"exec-fail-create");

        assert_ok!(Personal::propose_create(
            RuntimeOrigin::signed(c.clone()),
            account_name(b"exec-fail-create"),
            admins,
            2,
            CREATE_AMOUNT,
        ));
        let pid = last_proposal_id();

        // 模拟 fee_policy 在投票期变更后 ProposalData 中记录的快照费更高。
        // execute_create 只能按快照释放 reserve,因此会进入执行失败终态;
        // 终态回调随后必须按同一快照清理 Pending 和 reserve。
        overwrite_create_proposal_fee(pid, CREATE_FEE + 1);

        assert_ok!(cast_yes_votes(&admin_accounts[1..], 2, pid));
        process_pending_execution();

        let proposal = engingcore::Pallet::<Test>::proposals(pid).expect("proposal exists");
        assert_eq!(proposal.status, STATUS_EXECUTION_FAILED);
        assert_eq!(Balances::reserved_balance(&c), 0);
        assert!(!pallet::PendingPersonalCreate::<Test>::contains_key(pid));
        assert!(!pallet::PersonalAccounts::<Test>::contains_key(&dq));
        assert_eq!(create_failed_event_count(pid), 1);
    });
}

// ─── 13. 关闭执行失败 → 只清 PendingCloseProposal + 失败事件 ───────────

#[test]
fn close_execution_failed_terminal_keeps_account_and_clears_pending() {
    new_test_ext().execute_with(|| {
        let c = setup_creator_balance();
        let dq = proposed_account_id(&c, b"exec-fail-close");
        let admins_acc = vec![admin(0), admin(1), admin(2)];
        seed_active_multisig(&dq, &c, &admins_acc, 1_000);

        assert_ok!(Personal::propose_close(
            RuntimeOrigin::signed(admin(0)),
            dq.clone(),
            beneficiary_account_id(),
        ));
        let pid = last_proposal_id();
        set_institution_can_spend(false);

        assert_ok!(cast_yes_votes(&admins_acc[1..], 2, pid));
        process_pending_execution();

        let proposal = engingcore::Pallet::<Test>::proposals(pid).expect("proposal exists");
        assert_eq!(proposal.status, STATUS_EXECUTION_FAILED);
        assert!(pallet::PersonalAccounts::<Test>::contains_key(&dq));
        assert!(!pallet::PendingCloseProposal::<Test>::contains_key(&dq));
        assert_eq!(close_failed_event_count(pid), 1);
    });
}

// ─── 14. propose_close 拒绝非个人多签账户 ─────────────────────────────

#[test]
fn propose_close_rejects_when_not_personal_account() {
    new_test_ext().execute_with(|| {
        // 没在 PersonalAccounts 表里的地址
        let stranger = AccountId32::new([0xCC; 32]);

        assert_noop!(
            Personal::propose_close(
                RuntimeOrigin::signed(admin(0)),
                stranger,
                beneficiary_account_id(),
            ),
            pallet::Error::<Test>::NotPersonalAccount
        );
    });
}

// ─── 15. 非 admin 不能投票 ────────────────────────────────────────────

#[test]
fn non_admin_cannot_propose_or_vote() {
    new_test_ext().execute_with(|| {
        let c = setup_creator_balance();

        // 非 admin 提案 propose_create 时,creator_account_id 不在 admins 列表 → PermissionDenied
        assert_noop!(
            Personal::propose_create(
                RuntimeOrigin::signed(c),
                account_name(b"x"),
                admin_records(vec![admin(1), admin(2), admin(3)]),
                2,
                CREATE_AMOUNT,
            ),
            pallet::Error::<Test>::PermissionDenied
        );

        // 准备一个走通的提案,然后非 admin 投票被引擎拒
        let c2 = AccountId32::new([0x77; 32]);
        let _ = Balances::deposit_creating(&c2, SEED_BALANCE);
        let admins = admins_vec(3);
        assert_ok!(Personal::propose_create(
            RuntimeOrigin::signed(admin(0)),
            account_name(b"y"),
            admins,
            2,
            CREATE_AMOUNT,
        ));
        let pid = last_proposal_id();
        let stranger = AccountId32::new([0xEE; 32]);
        assert!(matches!(
            <internalvote::Pallet<Test>>::do_internal_vote(
                stranger,
                pid,
                internalvote::InternalVoteTicketClaim::Personal,
                true,
            ),
            Err(DispatchError::Module(_))
        ));
    });
}

// ─── 17. 关闭后链不死账(Existential Deposit 保留) ────────────────────

#[test]
fn existential_deposit_is_preserved_after_close() {
    new_test_ext().execute_with(|| {
        let c = setup_creator_balance();
        let dq = proposed_account_id(&c, b"ed-check");
        let admins_acc = vec![admin(0), admin(1), admin(2)];
        seed_active_multisig(&dq, &c, &admins_acc, 500);
        let beneficiary_acc = beneficiary_account_id();

        assert_ok!(Personal::propose_close(
            RuntimeOrigin::signed(admin(0)),
            dq.clone(),
            beneficiary_acc.clone(),
        ));
        let pid = last_proposal_id();
        assert_ok!(cast_yes_votes(&admins_acc[1..], 2, pid));
        process_pending_execution();

        // 多签账户应已被销户(转出后余额 < ED 直接 reap),beneficiary_account_id 拿到剩余金额
        assert_eq!(Balances::free_balance(&dq), 0);
        // 500 - fee(10) = 490
        assert_eq!(Balances::free_balance(&beneficiary_acc), 490);
        // ED = 1,490 >= 1,链不死账
    });
}

// 合并原成员测试：unit.rs

#[test]
fn propose_admin_set_change_updates_personal_admins_and_threshold() {
    new_test_ext().execute_with(|| {
        let account = personal_account_id();
        let old_admins = admins(&[admin(0), admin(1), admin(2)]);
        seed_active_admin_account(account.clone(), old_admins.clone(), 2);

        let mut next_admins = admins(&[admin(0), admin(3), admin(4), admin(5)]);
        next_admins[1].family_name = Default::default();
        next_admins[1].given_name = Default::default();
        let mut expected_admins = next_admins.clone();
        expected_admins[1].family_name =
            crate::types::FamilyName::truncate_from("管理".as_bytes().to_vec());
        expected_admins[1].given_name =
            crate::types::GivenName::truncate_from("员".as_bytes().to_vec());
        assert_ok!(Personal::propose_admin_set_change(
            RuntimeOrigin::signed(admin(0)),
            PMUL,
            account.clone(),
            next_admins.clone(),
            3,
        ));
        let proposal_id = engingcore::Pallet::<Test>::next_proposal_id().saturating_sub(1);

        // 提案创建时发起人 admin(0) 已自动赞成，阈值 2 时只需再补一票。
        assert_ok!(internalvote::Pallet::<Test>::do_internal_vote(
            admin(1),
            proposal_id,
            internalvote::InternalVoteTicketClaim::Personal,
            true
        ));
        // 通过判定只入队；管理员集合更新由维护管线异步执行。
        <VotingEngine as Hooks<u64>>::on_initialize(System::block_number());

        let account_id = pallet::AdminAccounts::<Test>::get(account.clone())
            .expect("admin account should exist");

        assert_eq!(account_id.admins, expected_admins);
        assert_eq!(
            internalvote::ActivePersonalThresholds::<Test>::get(account),
            Some(3)
        );
    });
}

// 合并原成员测试：security.rs

#[test]
fn invalid_thresholds_duplicate_admins_and_small_sets_are_atomic() {
    new_test_ext().execute_with(|| {
        let account = personal_account_id();
        seed_active_admin_account(account.clone(), admins(&[admin(0), admin(1), admin(2)]), 2);
        for threshold in [0, 4] {
            assert_noop!(
                Personal::propose_admin_set_change(
                    RuntimeOrigin::signed(admin(0)),
                    PMUL,
                    account.clone(),
                    admins(&[admin(0), admin(3), admin(4)]),
                    threshold
                ),
                Error::<Test>::InvalidThreshold
            );
        }
        assert_noop!(
            Personal::propose_admin_set_change(
                RuntimeOrigin::signed(admin(0)),
                PMUL,
                account.clone(),
                admins(&[admin(0), admin(0), admin(3)]),
                2
            ),
            Error::<Test>::DuplicateAdmin
        );
        for list in [admins(&[]), admins(&[admin(0)])] {
            assert_noop!(
                Personal::propose_admin_set_change(
                    RuntimeOrigin::signed(admin(0)),
                    PMUL,
                    account.clone(),
                    list,
                    1
                ),
                Error::<Test>::InvalidAdminsLen
            );
        }
    });
}

#[test]
fn outsiders_wrong_kind_missing_account_and_reordered_same_set_are_rejected() {
    new_test_ext().execute_with(|| {
        let account = personal_account_id();
        seed_active_admin_account(account.clone(), admins(&[admin(0), admin(1), admin(2)]), 2);
        let next = admins(&[admin(0), admin(3), admin(4)]);
        assert_noop!(
            Personal::propose_admin_set_change(
                RuntimeOrigin::signed(admin(9)),
                PMUL,
                account.clone(),
                next.clone(),
                2
            ),
            Error::<Test>::PermissionDenied
        );
        assert_noop!(
            Personal::propose_admin_set_change(
                RuntimeOrigin::signed(admin(0)),
                *b"NRCG",
                account.clone(),
                next.clone(),
                2
            ),
            Error::<Test>::NotPersonalAccount
        );
        assert_noop!(
            Personal::propose_admin_set_change(
                RuntimeOrigin::signed(admin(0)),
                PMUL,
                admin(30),
                next,
                2
            ),
            Error::<Test>::PersonalNotFound
        );
        assert_noop!(
            Personal::propose_admin_set_change(
                RuntimeOrigin::signed(admin(0)),
                PMUL,
                account,
                admins(&[admin(2), admin(0), admin(1)]),
                2
            ),
            Error::<Test>::AdminSetUnchanged
        );
    });
}

// 合并原成员测试：integration.rs

#[test]
fn approved_change_waits_for_execution_and_cannot_execute_twice() {
    new_test_ext().execute_with(|| {
        let account = personal_account_id();
        let before = admins(&[admin(0), admin(1), admin(2)]);
        seed_active_admin_account(account.clone(), before.clone(), 2);
        let next = admins(&[admin(0), admin(3), admin(4), admin(5)]);
        assert_ok!(Personal::propose_admin_set_change(
            RuntimeOrigin::signed(admin(0)),
            PMUL,
            account.clone(),
            next.clone(),
            3
        ));
        let id = VotingEngine::next_proposal_id() - 1;
        assert_ok!(internalvote::Pallet::<Test>::do_internal_vote(
            admin(1),
            id,
            internalvote::InternalVoteTicketClaim::Personal,
            true
        ));
        assert_eq!(AdminAccounts::<Test>::get(&account).unwrap().admins, before);
        assert_eq!(
            internalvote::ActivePersonalThresholds::<Test>::get(&account),
            Some(2)
        );
        <VotingEngine as Hooks<u64>>::on_initialize(System::block_number());
        assert_eq!(AdminAccounts::<Test>::get(&account).unwrap().admins, next);
        assert_eq!(
            internalvote::ActivePersonalThresholds::<Test>::get(&account),
            Some(3)
        );
        let state = sp_io::storage::root(sp_runtime::StateVersion::V1);
        <VotingEngine as Hooks<u64>>::on_initialize(System::block_number());
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), state);
    });
}

// 合并原成员测试：recovery.rs

#[test]
fn unregistered_execution_action_cannot_mutate_roster_or_threshold() {
    new_test_ext().execute_with(|| {
        let account = personal_account_id();
        seed_active_admin_account(account.clone(), admins(&[admin(0), admin(1), admin(2)]), 2);
        let action = crate::types::AdminSetChangeAction {
            personal_account_id: account.clone(),
            admins: admins(&[admin(0), admin(3), admin(4)]),
            new_threshold: 2,
        };
        assert_noop!(
            Personal::try_execute_set_change_from_action(999, action),
            Error::<Test>::ProposalActionNotFound
        );
        assert_eq!(
            internalvote::ActivePersonalThresholds::<Test>::get(account),
            Some(2)
        );
    });
}

/// 合并只改变成员入口的位置；原创建、关闭参数及成员载荷顺序不得被改义。
#[test]
fn personal_call_encoding_keeps_fields_and_reserved_indices() {
    let admins: pallet::AdminsOf<Test> = [1u8, 2]
        .into_iter()
        .map(|value| crate::Admin {
            account_id: AccountId32::new([value; 32]),

            family_name: Default::default(),
            given_name: Default::default(),
        })
        .collect::<Vec<_>>()
        .try_into()
        .expect("two members fit");
    // Compact(2) + 两项个人成员三字段；姓、名备注均为空。
    let mut member_bytes = vec![8u8];
    for value in [1u8, 2] {
        member_bytes.extend_from_slice(&[value; 32]);
        member_bytes.extend_from_slice(&[0, 0]);
    }

    let create = RuntimeCall::Personal(crate::Call::<Test>::propose_create {
        account_name: account_name(b"abc"),
        admins: admins.clone(),
        regular_threshold: 2,
        amount: 111,
    });
    let mut create_bytes = vec![7u8, 0, 12, b'a', b'b', b'c'];
    create_bytes.extend_from_slice(&member_bytes);
    create_bytes.extend_from_slice(&2u32.to_le_bytes());
    create_bytes.extend_from_slice(&111u128.to_le_bytes());
    assert_eq!(create.encode(), create_bytes);

    let close = RuntimeCall::Personal(crate::Call::<Test>::propose_close {
        account_id: AccountId32::new([9; 32]),
        beneficiary_account_id: AccountId32::new([10; 32]),
    });
    let mut close_bytes = vec![7u8, 1];
    close_bytes.extend_from_slice(&[9; 32]);
    close_bytes.extend_from_slice(&[10; 32]);
    assert_eq!(close.encode(), close_bytes);

    let change = RuntimeCall::Personal(crate::Call::<Test>::propose_admin_set_change {
        institution_code: PMUL,
        account: AccountId32::new([9; 32]),
        admins,
        new_threshold: 2,
    });
    let mut change_bytes = vec![7u8, 3];
    change_bytes.extend_from_slice(&PMUL);
    change_bytes.extend_from_slice(&[9; 32]);
    change_bytes.extend_from_slice(&member_bytes);
    change_bytes.extend_from_slice(&2u32.to_le_bytes());
    assert_eq!(change.encode(), change_bytes);
    assert_eq!(RuntimeCall::decode(&mut &change_bytes[..]).unwrap(), change);

    assert!(RuntimeCall::decode(&mut &[7u8, 2][..]).is_err());
    change_bytes[0] = 29;
    change_bytes[1] = 0;
    assert!(RuntimeCall::decode(&mut &change_bytes[..]).is_err());
}

/// 成员记录和原生命周期事件/错误继续保留原字节编号。
#[test]
fn personal_state_and_lifecycle_codes_are_preserved() {
    assert_eq!(crate::AdminAccountKind::PersonalMultisig.encode(), vec![2]);
    assert_eq!(crate::AdminAccountStatus::Pending.encode(), vec![0]);
    assert_eq!(crate::AdminAccountStatus::Active.encode(), vec![1]);
    assert_eq!(crate::AdminAccountStatus::Closed.encode(), vec![2]);
    assert_eq!(crate::PersonalStatus::Pending.encode(), vec![0]);
    assert_eq!(crate::PersonalStatus::Active.encode(), vec![1]);
    assert_eq!(
        crate::Event::<Test>::CloseExecutionFailed {
            proposal_id: 0,
            account_id: AccountId32::new([9; 32]),
        }
        .encode()[0],
        6,
    );
    assert_eq!(crate::Error::<Test>::NotPersonalAccount.encode()[0], 29);
}

/// 姓名备注进入成员记录，但成员编码不含CID，账户派生只取创建钱包与账户名。
#[test]
fn member_notes_do_not_change_personal_address_and_have_no_cid_slot() {
    let wallet = admin(1);
    let member = crate::Admin {
        account_id: wallet.clone(),
        family_name: b"note".to_vec().try_into().unwrap(),
        given_name: b"one".to_vec().try_into().unwrap(),
    };
    let mut expected = wallet.encode();
    expected.extend(member.family_name.encode());
    expected.extend(member.given_name.encode());
    assert_eq!(member.encode(), expected);
    let before = Personal::derive_personal_account(&wallet, b"shared").unwrap();
    let renamed = crate::Admin {
        given_name: b"two".to_vec().try_into().unwrap(),
        ..member
    };
    assert_eq!(
        Personal::derive_personal_account(&renamed.account_id, b"shared").unwrap(),
        before
    );
}

#[test]
fn threshold_only_change_uses_old_votes_and_activates_only_after_execution() {
    new_test_ext().execute_with(|| {
        let account = personal_account_id();
        let roster = admins(&[admin(0), admin(1), admin(2)]);
        seed_active_admin_account(account.clone(), roster.clone(), 2);
        assert_ok!(Personal::propose_admin_set_change(
            RuntimeOrigin::signed(admin(0)),
            PMUL,
            account.clone(),
            roster.clone(),
            1
        ));
        let id = engingcore::NextProposalId::<Test>::get() - 1;
        assert_eq!(
            internalvote::InternalThresholdSnapshot::<Test>::get(id),
            Some(2)
        );
        assert_eq!(
            internalvote::ActivePersonalThresholds::<Test>::get(&account),
            Some(2)
        );
        assert_ok!(InternalVote::cast(
            RuntimeOrigin::signed(admin(1)),
            id,
            internalvote::InternalVoteTicketClaim::Personal,
            true
        ));
        assert_eq!(
            internalvote::ActivePersonalThresholds::<Test>::get(&account),
            Some(2)
        );
        let now = System::block_number();
        <VotingEngine as frame_support::traits::Hooks<u64>>::on_initialize(now);
        assert_eq!(
            internalvote::ActivePersonalThresholds::<Test>::get(&account),
            Some(1)
        );
        assert_eq!(
            Personal::admin_account_of(account.clone()).unwrap().admins,
            roster
        );
        assert_noop!(
            Personal::propose_admin_set_change(
                RuntimeOrigin::signed(admin(0)),
                PMUL,
                account,
                roster,
                1
            ),
            Error::<Test>::AdminSetUnchanged
        );
    });
}

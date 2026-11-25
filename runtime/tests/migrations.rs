//! 使用旧链 SCALE 布局和本机最终状态副本排演机构升级。

use super::*;
use crate::migrations::{
    add_key, prefix, storage_key, Counts, Institutions, OldAssignment, OldInstitution, OldRole,
    RESULT,
};
use codec::{DecodeAll, Encode};
use frame_support::{
    migrations::MultiStepMigrator,
    storage::unhashed,
    traits::{OnRuntimeUpgrade, StorageVersion},
    weights::Weight,
    BoundedVec,
};
use primitives::{
    account_derive::{RESERVED_NAME_FEE, RESERVED_NAME_MAIN},
    institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE,
};
use serde_json::Value;

const OLD: &[u8] = b"PublicManage";
const GENESIS: &str = "0x18847a5dfd263272f2e7727836fe6582f8c4463ff48609df7b96d5e4d9dd24dd";
const COMMITTEE: &[u8] = primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER;

fn fixture() -> (
    publicentity::CidNumberOf<Runtime>,
    AccountId,
    identity::CidNumberBound,
) {
    let entry = &CHINA_CB[0];
    let cid = test_cid_number(entry.cid_number.as_bytes());
    let wallet = AccountId::new([19; 32]);
    let person = fixture_person(&wallet, "CTZN");
    let role: publicentity::RoleCodeOf = COMMITTEE.to_vec().try_into().unwrap();
    let source = entitybase::InstitutionAssignmentSource::Genesis;
    let status = entitybase::InstitutionRoleStatus::Active;
    let assignment = |account_id| OldAssignment {
        cid_number: cid.clone(),
        account_id,
        role_code: role.clone(),
        term_start: 0,
        term_end: 0,
        assignment_source: source,
        assignment_source_ref: Default::default(),
        assignment_status: entitybase::InstitutionAssignmentStatus::Active,
    };
    sp_io::storage::set(
        &storage_key(OLD, b"Institutions", &cid),
        &OldInstitution {
            cid_full_name: b"NRC".to_vec().try_into().unwrap(),
            cid_short_name: b"NRC".to_vec().try_into().unwrap(),
            town_code: Default::default(),
            legal_representative: None,
            institution_code: primitives::cid::code::NRC,
            created_at: 0,
        }
        .encode(),
    );
    let mut role_key = storage_key(OLD, b"InstitutionRoles", &cid);
    add_key(&mut role_key, &role);
    sp_io::storage::set(
        &role_key,
        &OldRole {
            cid_number: cid.clone(),
            role_code: role.clone(),
            role_name: b"committee".to_vec().try_into().unwrap(),
            term_required: false,
            role_status: status,
        }
        .encode(),
    );
    let mut assigned_key = storage_key(OLD, b"InstitutionRoleAssignments", &cid);
    add_key(&mut assigned_key, &role);
    sp_io::storage::set(
        &assigned_key,
        &vec![
            assignment(wallet.clone()),
            assignment(AccountId::new([20; 32])),
        ]
        .encode(),
    );
    for (name, account) in [
        (RESERVED_NAME_MAIN, entry.main_account),
        (RESERVED_NAME_FEE, entry.fee_account),
    ] {
        let name: publicentity::AccountNameOf<Runtime> = name.to_vec().try_into().unwrap();
        let account = AccountId::new(account);
        let mut key = storage_key(OLD, b"InstitutionAccounts", &cid);
        add_key(&mut key, &name);
        sp_io::storage::set(
            &key,
            &publicentity::InstitutionAccountInfo {
                account_id: account.clone(),
                initial_balance: 0u128,
                created_at: 0u32,
            }
            .encode(),
        );
        sp_io::storage::set(
            &storage_key(OLD, b"AccountRegisteredCid", &account),
            &entitybase::RegisteredInstitution {
                cid_number: cid.clone(),
                account_name: name,
            }
            .encode(),
        );
    }
    sp_io::storage::set(&prefix(b"PublicAdmins", b":__STORAGE_VERSION__:"), &[0, 0]);
    StorageVersion::new(1).put::<VotingEngine>();
    unhashed::put(b":citizenchain:prc-fee-sweep-permissions:v1", &true);
    (cid, wallet, person)
}

fn finish(limit: usize) -> usize {
    for block in 1..=limit {
        if !Institutions::ongoing() {
            return block - 1;
        }
        let progress = sp_io::storage::get(crate::migrations::PROGRESS);
        let weight = Institutions::step();
        assert_ne!(weight, Weight::zero());
        assert_ne!(
            sp_io::storage::get(crate::migrations::PROGRESS),
            progress,
            "迁移游标停在某条旧记录"
        );
    }
    panic!("迁移未在测试步数内完成");
}

#[test]
fn entity_migration_preserves_indexes_and_only_resolves_verified_people() {
    sp_io::TestExternalities::default().execute_with(|| {
        let (cid, wallet, person) = fixture();
        let historical = prefix(b"VotingEngine", b"VoterSnapshot");
        sp_io::storage::set(&historical, "原冻结钱包快照".as_bytes());
        Institutions::on_runtime_upgrade();
        assert!(Institutions::ongoing());
        assert_eq!(
            Institutions::step_with_budget(Weight::zero()),
            Weight::zero()
        );
        finish(80);
        assert!(!sp_io::storage::exists(&storage_key(
            OLD,
            b"Institutions",
            &cid
        )));
        let committee: publicentity::RoleCodeOf = COMMITTEE.to_vec().try_into().unwrap();
        let lr: publicentity::RoleCodeOf =
            ROLE_CODE_LEGAL_REPRESENTATIVE.to_vec().try_into().unwrap();
        let assignments =
            publicentity::InstitutionRoleAssignments::<Runtime>::get(&cid, &committee);
        assert_eq!(assignments.len(), 1);
        assert_eq!(assignments[0].person_cid, person);
        assert_eq!(
            publicentity::InstitutionRoles::<Runtime>::get(&cid, &lr)
                .unwrap()
                .seats,
            1
        );
        assert!(publicentity::InstitutionRoleAssignments::<Runtime>::get(&cid, &lr).is_empty());
        assert_eq!(Identity::person_account(&person), Some(wallet.clone()));
        assert_eq!(System::sufficients(&wallet), 0);
        assert_eq!(Balances::free_balance(&wallet), 0);
        assert_eq!(
            StorageVersion::get::<PublicEntity>(),
            StorageVersion::new(1)
        );
        assert_eq!(
            unhashed::get::<Counts>(RESULT).unwrap(),
            Counts {
                public: 1,
                accounts: 2,
                assignments: 1,
                missing_people: 1,
                ..Default::default()
            }
        );
        assert_eq!(
            sp_io::storage::get(&historical).unwrap().as_ref(),
            "原冻结钱包快照".as_bytes()
        );
        let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
        Institutions::on_runtime_upgrade();
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
    });
}

#[test]
fn entity_migration_stops_at_invalid_row_without_writing_its_target() {
    sp_io::TestExternalities::default().execute_with(|| {
        let (cid, _, _) = fixture();
        let role: publicentity::RoleCodeOf = COMMITTEE.to_vec().try_into().unwrap();
        let mut key = storage_key(OLD, b"InstitutionRoles", &cid);
        add_key(&mut key, &role);
        sp_io::storage::set(&key, &[255]);
        Institutions::on_runtime_upgrade();
        let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
        Institutions::step();
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
        assert!(!publicentity::InstitutionRoles::<Runtime>::contains_key(
            &cid, &role
        ));
        assert!(Institutions::ongoing());
        assert_eq!(sp_io::storage::get(&key).unwrap().as_ref(), &[255]);
    });
}

#[test]
fn entity_migration_resumes_after_state_commit() {
    let mut ext = sp_io::TestExternalities::default();
    ext.execute_with(|| {
        fixture();
        Institutions::on_runtime_upgrade();
        let budget = Weight::from_parts(20_000_000_000, 30_000);
        assert_ne!(Institutions::step_with_budget(budget), Weight::zero());
        assert!(Institutions::ongoing());
    });
    ext.commit_all().unwrap();
    ext.execute_with(|| {
        let progress = sp_io::storage::get(crate::migrations::PROGRESS);
        Institutions::on_runtime_upgrade();
        assert_eq!(sp_io::storage::get(crate::migrations::PROGRESS), progress);
        finish(80);
        assert_eq!(unhashed::get::<Counts>(RESULT).unwrap().public, 1);
    });
}

#[test]
fn entity_migration_rejects_active_legacy_proposals_before_writing() {
    sp_io::TestExternalities::default().execute_with(|| {
        fixture();
        let key = storage_key(b"VotingEngine", b"ActiveProposalsBySubject", &0u8);
        sp_io::storage::set(&key, &vec![7u64].encode());
        let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
        let result = std::panic::catch_unwind(Institutions::on_runtime_upgrade);
        assert!(result.is_err());
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
        assert!(!Institutions::ongoing());
    });
}

fn idle_proposal(status: u8, personal: bool) -> engingcore::Proposal<BlockNumber, AccountId> {
    engingcore::Proposal {
        kind: engingcore::PROPOSAL_KIND_INTERNAL,
        stage: engingcore::STAGE_INTERNAL,
        status,
        internal_code: Some(if personal {
            engingcore::types::PMUL
        } else {
            primitives::cid::code::NRC
        }),
        actor_cid_number: (!personal).then(|| test_cid_number(CHINA_CB[0].cid_number.as_bytes())),
        execution_account_id: personal.then(|| AccountId::new([81; 32])),
        subject_cid_numbers: Default::default(),
        start: 1,
        end: 100,
    }
}

fn reject_preflight_without_changes() {
    let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
    assert!(std::panic::catch_unwind(Institutions::on_runtime_upgrade).is_err());
    assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
    assert!(!Institutions::ongoing());
}

#[test]
fn entity_preflight_rejects_passed_proposal_after_engine_releases_active_index() {
    sp_io::TestExternalities::default().execute_with(|| {
        fixture();
        System::set_block_number(1);
        let proposal = idle_proposal(engingcore::STATUS_VOTING, false);
        let subject = proposal.subject_keys()[0].clone();
        engingcore::Proposals::<Runtime>::insert(7, proposal);
        engingcore::ActiveProposalsBySubject::<Runtime>::insert(
            &subject,
            BoundedVec::<u128, <Runtime as engingcore::Config>::MaxActiveProposals>::try_from(
                vec![7],
            )
            .unwrap(),
        );
        assert_ok!(VotingEngine::set_status_and_emit(
            7,
            engingcore::STATUS_PASSED
        ));
        assert!(engingcore::ActiveProposalsBySubject::<Runtime>::get(&subject).is_empty());
        assert!(engingcore::PendingProposalExecutions::<Runtime>::contains_key(7));
        reject_preflight_without_changes();
    });
}

#[test]
fn entity_preflight_rejects_unindexed_open_passed_or_invalid_proposals() {
    for status in [engingcore::STATUS_VOTING, engingcore::STATUS_PASSED, 255] {
        sp_io::TestExternalities::default().execute_with(|| {
            fixture();
            engingcore::Proposals::<Runtime>::insert(7, idle_proposal(status, false));
            reject_preflight_without_changes();
        });
    }
    sp_io::TestExternalities::default().execute_with(|| {
        fixture();
        sp_io::storage::set(
            &storage_key(b"VotingEngine", b"Proposals", &7u128),
            &[0, 0, engingcore::STATUS_EXECUTED],
        );
        reject_preflight_without_changes();
    });
}

#[test]
fn entity_preflight_rejects_orphan_and_terminal_mismatched_work_queues() {
    let pending = engingcore::PendingExecutionState {
        attempts: 0,
        next_attempt_at: 20u32,
    }
    .encode();
    for (table, value) in [
        (b"PendingProposalExecutions".as_slice(), pending.clone()),
        (
            b"ProposalExecutionRetryStates",
            engingcore::ExecutionRetryState {
                manual_attempts: 1,
                first_auto_failed_at: 1u32,
                retry_deadline: 20u32,
                last_attempt_at: None,
            }
            .encode(),
        ),
        (b"PendingExecutionRetryExpirations", 20u32.encode()),
        (b"PendingTerminalFinalizations", pending.clone()),
        (b"TerminalFinalizationDeadLetters", 3u8.encode()),
        (b"PendingTerminalCleanups", ().encode()),
        (b"AutoFinalizeRetryStates", pending),
        (b"AutoFinalizeDeadLetters", 3u8.encode()),
        (b"CallbackExecutionScopes", ().encode()),
        (
            b"PendingProposalCleanups",
            engingcore::PendingCleanupStage::TrackData.encode(),
        ),
        (b"PendingCleanupQueue", 7u128.encode()),
    ] {
        for status in [
            None,
            Some(engingcore::STATUS_EXECUTED),
            Some(engingcore::STATUS_EXECUTION_FAILED),
        ] {
            sp_io::TestExternalities::default().execute_with(|| {
                fixture();
                if let Some(status) = status {
                    engingcore::Proposals::<Runtime>::insert(7, idle_proposal(status, false));
                }
                let key = if table == b"PendingCleanupQueue" {
                    let encoded = 0u64.encode();
                    let mut key = prefix(b"VotingEngine", table).to_vec();
                    key.extend_from_slice(&sp_io::hashing::twox_64(&encoded));
                    key.extend(encoded);
                    key
                } else {
                    storage_key(b"VotingEngine", table, &7u128)
                };
                sp_io::storage::set(&key, &value);
                reject_preflight_without_changes();
            });
        }
    }
}

#[test]
fn entity_preflight_preserves_terminal_history_and_valid_retry_deadline_references() {
    for legacy_ids in [false, true] {
        sp_io::TestExternalities::default().execute_with(|| {
            fixture();
            StorageVersion::new(if legacy_ids { 0 } else { 1 }).put::<VotingEngine>();
            let mut saved = Vec::new();
            for (id, status) in [
                engingcore::STATUS_EXECUTED,
                engingcore::STATUS_REJECTED,
                engingcore::STATUS_EXECUTION_FAILED,
            ]
            .into_iter()
            .enumerate()
            {
                let key = if legacy_ids {
                    storage_key(b"VotingEngine", b"Proposals", &(id as u64))
                } else {
                    storage_key(b"VotingEngine", b"Proposals", &(id as u128))
                };
                let value = idle_proposal(status, true).encode();
                sp_io::storage::set(&key, &value);
                saved.push((key, value));
            }
            // 只有历史个人提案、票据和原保留期，不构成旧个人有效账户或待执行动作。
            let key = engingcore::ScheduledCleanups::<Runtime>::hashed_key_for(0u64);
            let value = if legacy_ids {
                (21703u32, 0u64).encode()
            } else {
                (21703u32, 0u128).encode()
            };
            sp_io::storage::set(&key, &value);
            saved.push((key, value));
            engingcore::ScheduledCleanupTail::<Runtime>::put(1u64);
            engingcore::PendingCleanupQueueHead::<Runtime>::put(5u64);
            engingcore::PendingCleanupQueueTail::<Runtime>::put(5u64);
            let key = storage_key(b"VotingEngine", b"ExecutionRetryDeadlines", &200u32);
            let value = if legacy_ids {
                vec![0u64, 1, 2].encode()
            } else {
                vec![0u128, 1, 2].encode()
            };
            sp_io::storage::set(&key, &value);
            saved.push((key, value));
            let key = prefix(b"VotingEngine", b"VoterSnapshot").to_vec();
            let value = vec![[1u8; 32]; 19].encode();
            sp_io::storage::set(&key, &value);
            saved.push((key, value));
            for pallet in [b"PersonalManage".as_slice(), b"PersonalAdmins"] {
                let key = prefix(pallet, b":__STORAGE_VERSION__:").to_vec();
                let value = 0u16.encode();
                sp_io::storage::set(&key, &value);
                saved.push((key, value));
            }
            Institutions::on_runtime_upgrade();
            assert!(Institutions::ongoing());
            for (key, value) in saved {
                assert_eq!(sp_io::storage::get(&key).unwrap().as_ref(), &value);
            }
        });
    }
}

#[test]
fn entity_preflight_rejects_orphan_retry_deadline_and_unreleased_mutex() {
    sp_io::TestExternalities::default().execute_with(|| {
        fixture();
        engingcore::ExecutionRetryDeadlines::<Runtime>::insert(100,
            BoundedVec::<u128, <Runtime as engingcore::Config>::MaxExecutionRetryDeadlinesPerBlock>::try_from(vec![9]).unwrap());
        reject_preflight_without_changes();
    });
    sp_io::TestExternalities::default().execute_with(|| {
        fixture();
        let subject = engingcore::ProposalSubject::PersonalAccount(AccountId::new([81; 32]));
        assert_ok!(VotingEngine::acquire_internal_proposal_mutex(
            7,
            subject,
            engingcore::InternalProposalMutexKind::Regular
        ));
        reject_preflight_without_changes();
    });
    sp_io::TestExternalities::default().execute_with(|| {
        fixture();
        engingcore::PendingCleanupQueueTail::<Runtime>::put(1u64);
        reject_preflight_without_changes();
    });
}

#[test]
fn entity_preflight_has_a_shared_bounded_history_scan() {
    for count in [512u128, 513] {
        sp_io::TestExternalities::default().execute_with(|| {
            fixture();
            for id in 0..count {
                engingcore::Proposals::<Runtime>::insert(
                    id,
                    idle_proposal(engingcore::STATUS_EXECUTED, false),
                );
            }
            if count == 512 {
                Institutions::on_runtime_upgrade();
                assert!(Institutions::ongoing());
            } else {
                reject_preflight_without_changes();
            }
        });
    }
}

#[test]
fn entity_preflight_rejects_each_unmigrated_personal_business_table() {
    for with_entities in [false, true] {
        let account = AccountId::new([81; 32]);
        for key in [
            storage_key(b"PersonalManage", b"PersonalAccounts", &account),
            storage_key(b"PersonalManage", b"PendingPersonalCreate", &7u128),
            storage_key(b"PersonalManage", b"PendingCloseProposal", &account),
            storage_key(b"PersonalAdmins", b"AdminAccounts", &account),
            storage_key(b"InternalVote", b"ActivePersonalThresholds", &account),
            storage_key(b"InternalVote", b"PendingPersonalThresholds", &7u128),
            storage_key(
                b"InternalVote",
                b"PendingPersonalAdminChangeThresholds",
                &7u128,
            ),
        ] {
            sp_io::TestExternalities::default().execute_with(|| {
                if with_entities {
                    fixture();
                }
                sp_io::storage::set(&key, &[1]);
                reject_preflight_without_changes();
            });
        }
    }
}

#[test]
fn entity_preflight_allows_empty_legacy_personal_and_current_personal_after_migration() {
    sp_io::TestExternalities::default().execute_with(|| {
        for pallet in [b"PersonalManage".as_slice(), b"PersonalAdmins"] {
            sp_io::storage::set(&prefix(pallet, b":__STORAGE_VERSION__:"), &0u16.encode());
        }
        let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
        Institutions::on_runtime_upgrade();
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
    });
    sp_io::TestExternalities::default().execute_with(|| {
        StorageVersion::new(1).put::<PublicEntity>();
        let account = AccountId::new([81; 32]);
        let key = storage_key(b"Personal", b"PersonalAccounts", &account);
        sp_io::storage::set(&key, &[1]);
        internalvote::ActivePersonalThresholds::<Runtime>::insert(&account, 2);
        engingcore::Proposals::<Runtime>::insert(7, idle_proposal(engingcore::STATUS_VOTING, true));
        let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
        Institutions::on_runtime_upgrade();
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
    });
}

/// 迁移以有界钩子推进，普通签名交易仍可组成合法非空块，治理写入暂停。
#[test]
fn entity_migration_keeps_nonempty_blocks_and_rejects_governance_writes() {
    use sp_runtime::traits::Header as _;

    sp_io::TestExternalities::default().execute_with(|| {
        fixture();
        let header = Header::new(
            1,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        assert_eq!(
            Executive::initialize_block(&header),
            sp_runtime::ExtrinsicInclusionMode::AllExtrinsics,
        );
        assert!(Institutions::pending());
        assert!(RuntimeCallFilter::contains(&RuntimeCall::System(
            frame_system::Call::remark { remark: vec![] }
        )));
        assert!(RuntimeCallFilter::contains(&RuntimeCall::Onchain(
            onchain::pallet::Call::transfer_with_remark {
                beneficiary_account_id: AccountId::new([99; 32]),
                amount: 1_000,
                remark: Default::default()
            }
        )));
        assert!(!RuntimeCallFilter::contains(&RuntimeCall::Identity(
            identity::Call::self_occupy_cid {
                cid_number: test_cid_number(b"CN220-CTZN2-198805200-2026")
            }
        )));
        let progress = sp_io::storage::get(crate::migrations::PROGRESS);
        <crate::migrations::EvmBackfill as frame_support::traits::PostInherents>::post_inherents();
        assert_ne!(sp_io::storage::get(crate::migrations::PROGRESS), progress);
        finish(80);
        assert!(!Institutions::ongoing());
    });
}

/// 使用现有公民 CID，合成旧钱包任职及已换绑状态；不创建第二个创世公民。
fn founder_fixture() -> (identity::CidNumberBound, AccountId, AccountId) {
    use crate::migrations::OldRepresentative;
    use primitives::cid::china::{china_zf, citizenchain};
    let person = test_cid_number(citizenchain::LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER.as_bytes());
    let old = AccountId::new([71; 32]);
    let current = AccountId::new([72; 32]);
    identity::AccountIdByCid::<Runtime>::insert(&person, &current);
    identity::CidByAccountId::<Runtime>::insert(&current, &person);
    identity::BindingRevisionByCid::<Runtime>::insert(&person, 2);
    identity::CidRegistry::<Runtime>::insert(
        &person,
        identity::CidRecord {
            registrar_cid_number: Default::default(),
            commitment: [0; 32],
            residence_province_code: Default::default(),
            residence_city_code: Default::default(),
            status: identity::CidRecordStatus::Active,
            registered_at: 0,
            revoked_at: None,
        },
    );
    let security = china_zf::CHINA_ZF
        .iter()
        .find(|entry| {
            primitives::cid::code::institution_code_from_cid_number(entry.cid_number)
                == Some(primitives::cid::code::FSC)
        })
        .unwrap();
    let registry = china_zf::CHINA_ZF
        .iter()
        .find(|entry| {
            primitives::cid::code::institution_code_from_cid_number(entry.cid_number)
                == Some(primitives::cid::code::FRG)
        })
        .unwrap();
    let judicial = primitives::cid::china::china_sf::CHINA_SF
        .iter()
        .find(|entry| {
            primitives::cid::code::institution_code_from_cid_number(entry.cid_number)
                == Some(primitives::cid::code::NJD)
        })
        .unwrap();
    let foundation = &citizenchain::CITIZENCHAIN_FOUNDATION;
    for (public, raw, main, fee, roles, has_representative, threshold) in [
        (
            false,
            foundation.cid_number,
            foundation.main_account,
            foundation.fee_account,
            citizenchain::CITIZENCHAIN_FIXED_ROLES
                .iter()
                .map(|r| r.role_code.to_vec())
                .collect::<Vec<_>>(),
            true,
            2u32,
        ),
        (
            true,
            security.cid_number,
            security.main_account,
            security.fee_account,
            china_zf::FSC_FIXED_ROLES
                .iter()
                .map(|r| r.to_vec())
                .collect(),
            true,
            2,
        ),
        (
            true,
            CHINA_CB[0].cid_number,
            CHINA_CB[0].main_account,
            CHINA_CB[0].fee_account,
            vec![COMMITTEE.to_vec()],
            false,
            13,
        ),
        (
            true,
            registry.cid_number,
            registry.main_account,
            registry.fee_account,
            vec![
                primitives::governance_skeleton::province_commissioner_role_code(*b"GZ"),
                primitives::governance_skeleton::province_commissioner_role_code(*b"ZS"),
            ],
            false,
            3,
        ),
        (
            true,
            judicial.cid_number,
            judicial.main_account,
            judicial.fee_account,
            vec![
                primitives::governance_skeleton::ROLE_CODE_CONSTITUTION_GUARD.to_vec(),
                primitives::governance_skeleton::ROLE_CODE_CHIEF_JUSTICE.to_vec(),
                primitives::governance_skeleton::ROLE_CODE_DEPUTY_CHIEF_JUSTICE.to_vec(),
                primitives::governance_skeleton::ROLE_CODE_JUSTICE.to_vec(),
            ],
            false,
            10,
        ),
    ] {
        let pallet = if public {
            b"PublicManage".as_slice()
        } else {
            b"PrivateManage".as_slice()
        };
        let cid = test_cid_number(raw.as_bytes());
        sp_io::storage::set(
            &storage_key(pallet, b"Institutions", &cid),
            &OldInstitution {
                cid_full_name: b"institution".to_vec().try_into().unwrap(),
                cid_short_name: b"institution".to_vec().try_into().unwrap(),
                town_code: Default::default(),
                legal_representative: has_representative.then(|| OldRepresentative {
                    family_name: "程".as_bytes().to_vec().try_into().unwrap(),
                    given_name: "伟".as_bytes().to_vec().try_into().unwrap(),
                    cid_number: person.clone(),
                    account_id: old.clone(),
                }),
                institution_code: primitives::cid::code::institution_code_from_cid_number(raw)
                    .unwrap(),
                created_at: 0,
            }
            .encode(),
        );
        sp_io::storage::set(
            &storage_key(pallet, b"InstitutionGovernanceThresholds", &cid),
            &threshold.encode(),
        );
        for raw_role in roles {
            let role: publicentity::RoleCodeOf = raw_role.try_into().unwrap();
            let mut key = storage_key(pallet, b"InstitutionRoles", &cid);
            add_key(&mut key, &role);
            sp_io::storage::set(
                &key,
                &OldRole {
                    cid_number: cid.clone(),
                    role_code: role.clone(),
                    role_name: role.to_vec().try_into().unwrap(),
                    term_required: false,
                    role_status: entitybase::InstitutionRoleStatus::Active,
                }
                .encode(),
            );
            let rows: Vec<OldAssignment> = if has_representative {
                vec![OldAssignment {
                    cid_number: cid.clone(),
                    account_id: old.clone(),
                    role_code: role.clone(),
                    term_start: 0,
                    term_end: 0,
                    assignment_source: entitybase::InstitutionAssignmentSource::Genesis,
                    assignment_source_ref: Default::default(),
                    assignment_status: entitybase::InstitutionAssignmentStatus::Active,
                }]
            } else {
                vec![]
            };
            let mut key = storage_key(pallet, b"InstitutionRoleAssignments", &cid);
            add_key(&mut key, &role);
            sp_io::storage::set(&key, &rows.encode());
        }
        for (raw_name, raw_account) in [(RESERVED_NAME_MAIN, main), (RESERVED_NAME_FEE, fee)] {
            let name: publicentity::AccountNameOf<Runtime> = raw_name.to_vec().try_into().unwrap();
            let account = AccountId::new(raw_account);
            let mut key = storage_key(pallet, b"InstitutionAccounts", &cid);
            add_key(&mut key, &name);
            sp_io::storage::set(
                &key,
                &publicentity::InstitutionAccountInfo {
                    account_id: account.clone(),
                    initial_balance: 0u128,
                    created_at: 0u32,
                }
                .encode(),
            );
            sp_io::storage::set(
                &storage_key(pallet, b"AccountRegisteredCid", &account),
                &entitybase::RegisteredInstitution {
                    cid_number: cid.clone(),
                    account_name: name,
                }
                .encode(),
            );
        }
    }
    StorageVersion::new(1).put::<VotingEngine>();
    unhashed::put(b":citizenchain:prc-fee-sweep-permissions:v1", &true);
    (person, old, current)
}

fn assert_founder_assignments(person: &identity::CidNumberBound, current: &AccountId) {
    use entitybase::InstitutionRoleQuery as _;
    let public: Vec<_> = publicentity::InstitutionRoleAssignments::<Runtime>::iter()
        .flat_map(|(_, _, rows)| rows.into_iter())
        .filter(|row| &row.person_cid == person)
        .collect();
    let private: Vec<_> = privateentity::InstitutionRoleAssignments::<Runtime>::iter()
        .flat_map(|(_, _, rows)| rows.into_iter())
        .filter(|row| &row.person_cid == person)
        .collect();
    assert_eq!((public.len(), private.len()), (9, 3));
    let judicial = primitives::governance_skeleton::fixed_institutions()
        .into_iter()
        .find(|entry| entry.code == primitives::cid::code::NJD)
        .unwrap();
    let registry = primitives::governance_skeleton::federal_registry_institution();
    let expected = [
        (CHINA_CB[0].cid_number, COMMITTEE.to_vec()),
        (
            registry.cid_number,
            primitives::governance_skeleton::province_commissioner_role_code(*b"GZ"),
        ),
        (
            registry.cid_number,
            primitives::governance_skeleton::province_commissioner_role_code(*b"ZS"),
        ),
        (
            judicial.cid_number,
            primitives::governance_skeleton::ROLE_CODE_CONSTITUTION_GUARD.to_vec(),
        ),
        (
            judicial.cid_number,
            primitives::governance_skeleton::ROLE_CODE_CHIEF_JUSTICE.to_vec(),
        ),
        (
            judicial.cid_number,
            primitives::governance_skeleton::ROLE_CODE_DEPUTY_CHIEF_JUSTICE.to_vec(),
        ),
        (
            judicial.cid_number,
            primitives::governance_skeleton::ROLE_CODE_JUSTICE.to_vec(),
        ),
    ];
    for (cid, code) in expected {
        let rows: Vec<_> = public
            .iter()
            .filter(|row| {
                row.cid_number.as_slice() == cid.as_bytes() && row.role_code.as_slice() == code
            })
            .collect();
        assert_eq!(rows.len(), 1, "每个接替岗位只能任职一席");
        assert_eq!(
            rows[0].assignment_source_ref.as_slice(),
            (System::block_hash(0), System::block_number()).encode()
        );
        let role =
            publicentity::InstitutionRoles::<Runtime>::get(&rows[0].cid_number, &rows[0].role_code)
                .unwrap();
        let info = publicentity::Institutions::<Runtime>::get(&rows[0].cid_number).unwrap();
        assert_eq!(
            Some(role.seats),
            primitives::governance_skeleton::fixed_role_seats_by_identity(
                info.institution_code,
                &rows[0].cid_number,
                &rows[0].role_code
            )
        );
    }
    assert!(public.iter().chain(private.iter()).all(|row| {
        row.term_start == 0
            && row.term_end == 0
            && row.assignment_source == entitybase::InstitutionAssignmentSource::Genesis
            && row.assignment_status == entitybase::InstitutionAssignmentStatus::Active
    }));
    assert_eq!(
        public
            .iter()
            .filter(|row| !row.assignment_source_ref.is_empty())
            .count(),
        7
    );
    assert!(private
        .iter()
        .all(|row| row.assignment_source_ref.is_empty()));
    for row in &public {
        assert!(PublicEntity::is_active_assignment(
            &row.cid_number,
            current,
            &row.role_code
        ));
        if primitives::cid::code::institution_code_from_cid_number(
            core::str::from_utf8(&row.cid_number).unwrap(),
        ) == Some(primitives::cid::code::FSC)
        {
            assert_eq!(
                publicentity::InstitutionRoles::<Runtime>::get(&row.cid_number, &row.role_code)
                    .unwrap()
                    .seats,
                1
            );
        }
    }
    for row in &private {
        assert!(PrivateEntity::is_active_assignment(
            &row.cid_number,
            current,
            &row.role_code
        ));
    }
    let representatives: Vec<_> = publicentity::Institutions::<Runtime>::iter()
        .filter_map(|(_, info)| info.legal_representative)
        .chain(
            privateentity::Institutions::<Runtime>::iter()
                .filter_map(|(_, info)| info.legal_representative),
        )
        .filter(|representative| &representative.person_cid == person)
        .collect();
    assert_eq!(representatives.len(), 2);
    assert!(representatives.iter().all(|row| &row.person_cid == person));
    let nrc = test_cid_number(CHINA_CB[0].cid_number.as_bytes());
    let role: publicentity::RoleCodeOf = COMMITTEE.to_vec().try_into().unwrap();
    assert_eq!(
        publicentity::InstitutionRoles::<Runtime>::get(&nrc, &role)
            .unwrap()
            .seats,
        19
    );
    assert_eq!(
        publicentity::InstitutionGovernanceThresholds::<Runtime>::get(&nrc),
        Some(13)
    );
    let registry = primitives::governance_skeleton::federal_registry_institution();
    let cid = test_cid_number(registry.cid_number.as_bytes());
    let role: publicentity::RoleCodeOf =
        primitives::governance_skeleton::province_commissioner_role_code(*b"GZ")
            .try_into()
            .unwrap();
    assert_eq!(
        publicentity::InstitutionRoles::<Runtime>::get(&cid, &role)
            .unwrap()
            .seats,
        5
    );
}

#[test]
fn entity_migration_keeps_five_rebound_assignments_and_adds_seven_seats_once() {
    sp_io::TestExternalities::default().execute_with(|| {
        let (person, old, current) = founder_fixture();
        System::set_block_number(726);
        // 旧钱包即使后来属于另一人员，也不得把五条原任职转给该人员。
        let other = fixture_person(&old, "CTZN");
        assert_ne!(other, person);
        genesis::Phase::<Runtime>::put(genesis::ChainPhase::Operation);
        genesis::DeveloperUpgradeEnabled::<Runtime>::put(false);
        let historical = prefix(b"VotingEngine", b"VoterSnapshot");
        sp_io::storage::set(&historical, &[19, 15, 13, 2]);
        let issuance = Balances::total_issuance();
        let account = System::account(&current);
        Institutions::on_runtime_upgrade();
        finish(80);
        assert_founder_assignments(&person, &current);
        assert_eq!(unhashed::get::<Counts>(RESULT).unwrap().assignments, 12);
        assert!(is_nrc_member(&current));
        assert!(!is_nrc_member(&old));
        assert_eq!(Identity::person_account(&person), Some(current.clone()));
        assert_eq!(
            identity::BindingRevisionByCid::<Runtime>::get(&person),
            Some(2)
        );
        assert_eq!(System::account(&current), account);
        assert_eq!(Balances::total_issuance(), issuance);
        assert_eq!(
            sp_io::storage::get(&historical).unwrap().as_ref(),
            &[19, 15, 13, 2]
        );
        assert_eq!(
            genesis::Phase::<Runtime>::get(),
            genesis::ChainPhase::Operation
        );
        assert!(!genesis::DeveloperUpgradeEnabled::<Runtime>::get());
        let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
        Institutions::on_runtime_upgrade();
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
    });
}

#[test]
fn migration_guard_contract_is_read_only_and_rejects_corrupt_or_false_completion() {
    sp_io::TestExternalities::default().execute_with(|| {
        founder_fixture();
        let read = |key: &[u8]| sp_io::storage::get(key).map(|v| v.to_vec());
        assert_eq!(
            crate::guard_migration_state(read).unwrap(),
            crate::MigrationState::NotStarted
        );
        Institutions::on_runtime_upgrade();
        let progress = sp_io::storage::get(crate::migrations::PROGRESS).unwrap();
        let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
        assert_eq!(
            crate::guard_migration_state(read).unwrap(),
            crate::MigrationState::Running
        );
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
        for corrupt in [
            progress[..progress.len() - 1].to_vec(),
            [progress.as_ref(), &[0]].concat(),
            {
                let mut bytes = progress.to_vec();
                bytes[0] = 255;
                bytes
            },
        ] {
            sp_io::storage::set(crate::migrations::PROGRESS, &corrupt);
            assert!(crate::guard_migration_state(read).is_err());
        }
        sp_io::storage::set(crate::migrations::PROGRESS, &progress);
        finish(80);
        assert_eq!(
            crate::guard_migration_state(read).unwrap(),
            crate::MigrationState::Complete
        );
        sp_io::storage::set(crate::migrations::PROGRESS, &progress);
        assert!(crate::guard_migration_state(read).is_err());
        sp_io::storage::clear(crate::migrations::PROGRESS);
        StorageVersion::new(0).put::<PrivateEntity>();
        assert!(crate::guard_migration_state(read).is_err());
    });
}

#[test]
fn migration_guard_institution_uses_explicit_layout_and_never_restores_wallet_authority() {
    sp_io::TestExternalities::default().execute_with(|| {
        let (person, old, current) = founder_fixture();
        let cid = test_cid_number(
            primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION
                .cid_number
                .as_bytes(),
        );
        let raw =
            sp_io::storage::get(&storage_key(b"PrivateManage", b"Institutions", &cid)).unwrap();
        let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
        let facts = crate::guard_institution(&raw, true).unwrap();
        assert_eq!(
            facts.legal_representative.as_ref().unwrap().person_cid,
            person.to_vec()
        );
        assert!(crate::guard_institution(&raw, false).is_err());
        assert_eq!(
            crate::guard_institution(&facts.encode(), false)
                .unwrap()
                .encode(),
            facts.encode()
        );
        assert!(crate::guard_institution(&facts.encode(), true).is_err());
        assert!(crate::guard_institution(&[raw.as_ref(), &[0]].concat(), true).is_err());
        assert_eq!(Identity::person_account(&person), Some(current));
        assert!(identity::CidByAccountId::<Runtime>::get(&old).is_none());
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
    });
}

#[test]
fn migration_guard_layout_uses_real_value_types_and_rejects_unknown_schema() {
    let mut registry = scale_info::Registry::new();
    let old = registry.register_type(&scale_info::meta_type::<crate::migrations::OldInstitution>());
    let current = registry.register_type(&scale_info::meta_type::<
        entitybase::InstitutionInfo<
            u32,
            publicentity::AccountNameOf<Runtime>,
            publicentity::CidNumberOf<Runtime>,
        >,
    >());
    let unrelated = registry.register_type(&scale_info::meta_type::<u64>());
    let portable = registry.into();
    assert_eq!(crate::guard_institution_layout(&portable, old.id), Ok(true));
    assert_eq!(
        crate::guard_institution_layout(&portable, current.id),
        Ok(false)
    );
    assert!(crate::guard_institution_layout(&portable, unrelated.id).is_err());
    assert!(crate::guard_institution_layout(&portable, u32::MAX).is_err());
}

#[test]
fn migration_guard_entity_versions_accept_only_frame_initialized_targets() {
    use frame_support::traits::BeforeAllRuntimeMigrations;
    sp_io::TestExternalities::default().execute_with(|| {
        AllPalletsWithSystem::before_all_runtime_migrations();
        let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
        for (public, name) in [
            (true, b"PublicEntity".as_slice()),
            (false, b"PrivateEntity".as_slice()),
        ] {
            let initialized = sp_io::storage::get(&prefix(name, b":__STORAGE_VERSION__:")).unwrap();
            assert_eq!(
                crate::guard_entity_versions(public, &0u16.encode(), &initialized),
                Ok(())
            );
            assert!(crate::guard_entity_versions(public, &[0], &initialized).is_err());
            assert!(crate::guard_entity_versions(public, &0u16.encode(), &2u16.encode()).is_err());
            assert!(crate::guard_entity_versions(public, &0u16.encode(), &[0]).is_err());
        }
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
    });
}

#[test]
fn migration_guard_grandpa_namespace_keeps_schema_changes_visible_and_rejects_unknown_tables() {
    let key = prefix(b"GrandpaKeyChange", b"GrandpaKeyOwnerByKey");
    let expected = prefix(b"GrandpaKey", b"GrandpaKeyOwnerByKey");
    assert_eq!(
        crate::guard_migration_target(&key),
        Ok(Some(expected.to_vec()))
    );
    assert!(crate::guard_retired_key(&key));
    assert!(!crate::guard_retired_key(&expected));
    assert!(crate::guard_migration_target(&prefix(b"GrandpaKeyChange", b"Unknown")).is_err());
    assert!(crate::guard_migration_target(&sp_io::hashing::twox_128(b"GrandpaKeyChange")).is_err());
}

#[test]
fn migration_guard_phase_transition_only_advances_and_freezes_completion() {
    sp_io::TestExternalities::default().execute_with(|| {
        founder_fixture();
        let snapshot = || {
            [
                crate::migrations::PROGRESS.to_vec(),
                RESULT.to_vec(),
                prefix(b"PublicEntity", b":__STORAGE_VERSION__:").to_vec(),
                prefix(b"PrivateEntity", b":__STORAGE_VERSION__:").to_vec(),
            ]
            .into_iter()
            .filter_map(|key| sp_io::storage::get(&key).map(|value| (key, value.to_vec())))
            .collect::<std::collections::BTreeMap<_, _>>()
        };
        let before = snapshot();
        Institutions::on_runtime_upgrade();
        let start = snapshot();
        Institutions::step();
        let middle = snapshot();
        assert_ne!(start, middle);
        finish(80);
        let complete = snapshot();
        let check = |a: &std::collections::BTreeMap<Vec<u8>, Vec<u8>>,
                     b: &std::collections::BTreeMap<Vec<u8>, Vec<u8>>| {
            crate::guard_migration_transition(|k| a.get(k).cloned(), |k| b.get(k).cloned())
        };
        assert_eq!(check(&before, &start), Ok(()));
        assert_eq!(check(&start, &middle), Ok(()));
        assert_eq!(check(&middle, &complete), Ok(()));
        assert_eq!(check(&complete, &complete), Ok(()));
        assert!(check(&middle, &start).is_err());
        assert!(check(&middle, &before).is_err());
        assert!(check(&complete, &middle).is_err());
        let mut forged = complete.clone();
        let mut counts = Counts::decode_all(&mut complete[RESULT].as_slice()).unwrap();
        counts.missing_people += 1;
        forged.insert(RESULT.to_vec(), counts.encode());
        assert!(check(&complete, &forged).is_err());
    });
}

#[test]
fn migration_guard_checks_real_partial_assignment_batches_and_rejects_shadow_rows() {
    sp_io::TestExternalities::default().execute_with(|| {
        fixture();
        let spec = primitives::institution_constraints::member_composition_specs()[0];
        let cid = test_cid_number(spec.institution.cid_number.as_bytes());
        let role: publicentity::RoleCodeOf = spec.role_code.to_vec().try_into().unwrap();
        let mut role_key = storage_key(OLD, b"InstitutionRoles", &cid);
        add_key(&mut role_key, &role);
        sp_io::storage::set(
            &role_key,
            &OldRole {
                cid_number: cid.clone(),
                role_code: role.clone(),
                role_name: spec.role_name.to_vec().try_into().unwrap(),
                term_required: false,
                role_status: entitybase::InstitutionRoleStatus::Active,
            }
            .encode(),
        );
        let rows: Vec<_> = (0..spec.min_members)
            .map(|index| {
                let mut account = [0u8; 32];
                account[..4].copy_from_slice(&(index + 1000).to_le_bytes());
                OldAssignment {
                    cid_number: cid.clone(),
                    account_id: AccountId::new(account),
                    role_code: role.clone(),
                    term_start: 0,
                    term_end: 0,
                    assignment_source: entitybase::InstitutionAssignmentSource::Genesis,
                    assignment_source_ref: Default::default(),
                    assignment_status: entitybase::InstitutionAssignmentStatus::Active,
                }
            })
            .collect();
        let bound = fixture_person(&rows[0].account_id, "CTZN");
        let mut source = storage_key(OLD, b"InstitutionRoleAssignments", &cid);
        add_key(&mut source, &role);
        let mut target = storage_key(b"PublicEntity", b"InstitutionRoleAssignments", &cid);
        add_key(&mut target, &role);
        sp_io::storage::set(&source, &rows.encode());
        Institutions::on_runtime_upgrade();
        let mut observed = false;
        for _ in 0..12 {
            Institutions::step();
            if let (Some(old), Some(new)) =
                (sp_io::storage::get(&source), sp_io::storage::get(&target))
            {
                observed = true;
                let read = |key: &[u8]| Ok(sp_io::storage::get(key).map(|v| v.to_vec()));
                let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
                assert_eq!(
                    crate::guard_migration_state(|k| sp_io::storage::get(k).map(|v| v.to_vec())),
                    Ok(crate::MigrationState::Running)
                );
                assert_eq!(
                    crate::guard_partial_assignments(&target, &old, &new, read),
                    Ok(())
                );
                let current: Vec<publicentity::InstitutionAssignmentOf<Runtime>> =
                    DecodeAll::decode_all(&mut new.as_ref()).unwrap();
                assert_eq!(current.len(), 1);
                assert_eq!(current[0].person_cid, bound);
                assert!(crate::guard_partial_assignments(&target, &old, &vec![0], read).is_err());
                assert!(crate::guard_partial_assignments(&source, &old, &new, read).is_err());
                assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
                break;
            }
        }
        assert!(observed, "真实105席任职产生有界中间状态");
    });
}

#[test]
fn entity_migration_rejects_invalid_founder_evidence_before_writing() {
    for failure in 0..4 {
        sp_io::TestExternalities::default().execute_with(|| {
            let (person, _, current) = founder_fixture();
            match failure {
                0 => identity::CidByAccountId::<Runtime>::remove(&current),
                1 => identity::CidRegistry::<Runtime>::mutate(&person, |record| {
                    record.as_mut().unwrap().status = identity::CidRecordStatus::Revoked
                }),
                2 => identity::CidRegistry::<Runtime>::mutate(&person, |record| {
                    record.as_mut().unwrap().registered_at = 1
                }),
                _ => {
                    let cid = test_cid_number(
                        primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION
                            .cid_number
                            .as_bytes(),
                    );
                    let key = storage_key(b"PrivateManage", b"Institutions", &cid);
                    let mut info = OldInstitution::decode_all(
                        &mut sp_io::storage::get(&key).unwrap().as_ref(),
                    )
                    .unwrap();
                    info.legal_representative.as_mut().unwrap().account_id =
                        AccountId::new([73; 32]);
                    sp_io::storage::set(&key, &info.encode());
                }
            }
            let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
            assert!(std::panic::catch_unwind(Institutions::on_runtime_upgrade).is_err());
            assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
            assert!(!Institutions::ongoing());
        });
    }
}

#[test]
fn entity_migration_founder_target_conflict_does_not_write_any_new_seat() {
    sp_io::TestExternalities::default().execute_with(|| {
        let (person, _, current) = founder_fixture();
        let cid = test_cid_number(CHINA_CB[0].cid_number.as_bytes());
        let role: publicentity::RoleCodeOf = COMMITTEE.to_vec().try_into().unwrap();
        let mut key = storage_key(OLD, b"InstitutionRoleAssignments", &cid);
        add_key(&mut key, &role);
        sp_io::storage::set(
            &key,
            &vec![OldAssignment {
                cid_number: cid,
                account_id: current.clone(),
                role_code: role,
                term_start: 0,
                term_end: 0,
                assignment_source: entitybase::InstitutionAssignmentSource::Genesis,
                assignment_source_ref: Default::default(),
                assignment_status: entitybase::InstitutionAssignmentStatus::Active,
            }]
            .encode(),
        );
        Institutions::on_runtime_upgrade();
        for _ in 0..20 {
            let before = sp_io::storage::get(crate::migrations::PROGRESS);
            Institutions::step();
            if before == sp_io::storage::get(crate::migrations::PROGRESS) {
                break;
            }
        }
        assert!(Institutions::ongoing());
        assert!(!sp_io::storage::exists(RESULT));
        let registry = test_cid_number(
            primitives::governance_skeleton::federal_registry_institution()
                .cid_number
                .as_bytes(),
        );
        let role: publicentity::RoleCodeOf =
            primitives::governance_skeleton::province_commissioner_role_code(*b"GZ")
                .try_into()
                .unwrap();
        assert!(
            publicentity::InstitutionRoleAssignments::<Runtime>::get(&registry, &role).is_empty()
        );
        let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
        Institutions::step();
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
        assert_eq!(Identity::person_account(&person), Some(current));
    });
}

#[test]
fn entity_migration_founder_assignments_resume_after_state_commit() {
    let mut ext = sp_io::TestExternalities::default();
    let (person, current) = ext.execute_with(|| {
        let (person, _, current) = founder_fixture();
        Institutions::on_runtime_upgrade();
        Institutions::step_with_budget(Weight::from_parts(20_000_000_000, 100_000));
        assert!(Institutions::ongoing());
        (person, current)
    });
    ext.commit_all().unwrap();
    ext.execute_with(|| {
        Institutions::on_runtime_upgrade();
        finish(80);
        assert_founder_assignments(&person, &current);
    });
}

#[test]
fn entity_migration_late_duplicate_or_full_seat_rolls_back_all_seven_replacements() {
    for duplicate in [true, false] {
        sp_io::TestExternalities::default().execute_with(|| {
            let (person, _, current) = founder_fixture();
            let judicial = primitives::governance_skeleton::fixed_institutions()
                .into_iter()
                .find(|entry| entry.code == primitives::cid::code::NJD)
                .unwrap();
            let cid = test_cid_number(judicial.cid_number.as_bytes());
            let role: publicentity::RoleCodeOf = if duplicate {
                primitives::governance_skeleton::ROLE_CODE_JUSTICE
            } else {
                primitives::governance_skeleton::ROLE_CODE_CHIEF_JUSTICE
            }
            .to_vec()
            .try_into()
            .unwrap();
            let account = if duplicate {
                current.clone()
            } else {
                AccountId::new([88; 32])
            };
            let retained_person = if duplicate {
                person.clone()
            } else {
                fixture_person(&account, "CTZN")
            };
            let mut key = storage_key(OLD, b"InstitutionRoleAssignments", &cid);
            add_key(&mut key, &role);
            sp_io::storage::set(
                &key,
                &vec![OldAssignment {
                    cid_number: cid.clone(),
                    account_id: account,
                    role_code: role.clone(),
                    term_start: 0,
                    term_end: 0,
                    assignment_source: entitybase::InstitutionAssignmentSource::Genesis,
                    assignment_source_ref: Default::default(),
                    assignment_status: entitybase::InstitutionAssignmentStatus::Active,
                }]
                .encode(),
            );
            Institutions::on_runtime_upgrade();
            for _ in 0..80 {
                let before = sp_io::storage::get(crate::migrations::PROGRESS);
                Institutions::step();
                if before == sp_io::storage::get(crate::migrations::PROGRESS) {
                    break;
                }
            }
            assert!(Institutions::ongoing());
            assert!(!sp_io::storage::exists(RESULT));
            let nrc = test_cid_number(CHINA_CB[0].cid_number.as_bytes());
            let committee: publicentity::RoleCodeOf = COMMITTEE.to_vec().try_into().unwrap();
            assert!(
                publicentity::InstitutionRoleAssignments::<Runtime>::get(&nrc, &committee)
                    .is_empty()
            );
            let registry = test_cid_number(
                primitives::governance_skeleton::federal_registry_institution()
                    .cid_number
                    .as_bytes(),
            );
            for province in [*b"GZ", *b"ZS"] {
                let r: publicentity::RoleCodeOf =
                    primitives::governance_skeleton::province_commissioner_role_code(province)
                        .try_into()
                        .unwrap();
                assert!(
                    publicentity::InstitutionRoleAssignments::<Runtime>::get(&registry, &r)
                        .is_empty()
                );
            }
            let retained = publicentity::InstitutionRoleAssignments::<Runtime>::get(&cid, &role);
            assert_eq!(retained.len(), 1);
            assert_eq!(retained[0].person_cid, retained_person);
            let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
            Institutions::step();
            assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
        });
    }
}

#[test]
#[ignore = "通过塔塔控制台显式传入最终区块的只读状态副本"]
fn entity_migration_replays_finalized_chain_state() {
    let path = std::env::var_os("CITIZENCHAIN_ENTITY_SNAPSHOT").expect("需要最终状态副本");
    let snapshot: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(snapshot["genesis"].as_str(), Some(GENESIS));
    let rows = snapshot["rows"].as_array().unwrap();
    let bytes =
        |value: &Value| hex::decode(value.as_str().unwrap().strip_prefix("0x").unwrap()).unwrap();
    let version = sp_runtime::StateVersion::try_from(
        snapshot["version"]["stateVersion"].as_u64().unwrap() as u8,
    )
    .unwrap();
    let storage = sp_core::storage::Storage {
        top: rows
            .iter()
            .map(|row| (bytes(&row[0]), bytes(&row[1])))
            .collect(),
        ..Default::default()
    };
    let mut ext = sp_io::TestExternalities::new_with_state_version(storage, version);
    // 使用锁定SDK真实证明记录器只读枚举全部来源和目标；不绕过生产迁移预算。
    let ((scanned, payload), proof) = ext.execute_and_prove(|| {
        let mut scanned = 0usize;
        let mut payload = 0usize;
        for (source, target, _) in crate::renamed_pallets() {
            let from = sp_io::hashing::twox_128(source.as_bytes());
            let to = sp_io::hashing::twox_128(target.as_bytes());
            let mut cursor = from.to_vec();
            while let Some(key) = sp_io::storage::next_key(&cursor).filter(|k| k.starts_with(&from))
            {
                let value = sp_io::storage::get(&key).unwrap();
                payload += value.len();
                scanned += 1;
                let target = [to.as_slice(), &key[16..]].concat();
                let _ = sp_io::storage::get(&target);
                cursor = key;
            }
        }
        (scanned, payload)
    });
    eprintln!(
        "前缀改名只读证明：{scanned}条、{payload}值字节、{}证明字节",
        proof.encoded_size()
    );
    ext.execute_with(|| {
        let bytes = |value: &Value| {
            hex::decode(value.as_str().unwrap().strip_prefix("0x").unwrap()).unwrap()
        };
        for row in rows {
            sp_io::storage::set(&bytes(&row[0]), &bytes(&row[1]));
        }
        let version = sp_runtime::StateVersion::try_from(
            snapshot["version"]["stateVersion"].as_u64().unwrap() as u8,
        )
        .unwrap();
        assert_eq!(
            sp_io::storage::root(version),
            bytes(&snapshot["header"]["stateRoot"]),
            "完整只读副本必须先匹配最终区块状态根"
        );
        let genesis = Hash::from_slice(&bytes(&snapshot["genesis"]));
        frame_system::BlockHash::<Runtime>::insert(0, genesis);
        let issuance = Balances::total_issuance();
        let accounts: Vec<_> = frame_system::Account::<Runtime>::iter().collect();
        let history = |prefix: &[u8]| {
            let mut cursor = prefix.to_vec();
            let mut entries = Vec::new();
            while let Some(key) =
                sp_io::storage::next_key(&cursor).filter(|key| key.starts_with(prefix))
            {
                let value = sp_io::storage::get(&key).unwrap();
                entries.push((key.clone(), value));
                cursor = key;
            }
            entries
        };
        let source_public = history(&prefix(b"PublicManage", b"Institutions")).len();
        let source_private = history(&prefix(b"PrivateManage", b"Institutions")).len();
        let source_accounts = history(&prefix(b"PublicManage", b"InstitutionAccounts")).len()
            + history(&prefix(b"PrivateManage", b"InstitutionAccounts")).len();
        let foundation = test_cid_number(
            primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION
                .cid_number
                .as_bytes(),
        );
        let original =
            sp_io::storage::get(&storage_key(b"PrivateManage", b"Institutions", &foundation))
                .unwrap();
        let person = OldInstitution::decode_all(&mut original.as_ref())
            .unwrap()
            .legal_representative
            .unwrap()
            .cid_number;
        <Runtime as frame_system::Config>::SingleBlockMigrations::on_runtime_upgrade();
        // 原岗位中当前已绑定的人员逐条保留；创世原五职和七个接替岗位另行精确断言。
        let mut bound_rows = Vec::new();
        for public in [true, false] {
            let pallet = if public {
                b"PublicManage".as_slice()
            } else {
                b"PrivateManage".as_slice()
            };
            for (_, raw) in history(&prefix(pallet, b"InstitutionRoleAssignments")) {
                let rows: Vec<OldAssignment> = DecodeAll::decode_all(&mut raw.as_ref()).unwrap();
                for row in rows {
                    if let Some(person) = Identity::person_cid(&row.account_id) {
                        bound_rows.push((public, row, person));
                    }
                }
            }
        }
        let vote_prefix = sp_io::hashing::twox_128(b"VotingEngine");
        let internal_prefix = sp_io::hashing::twox_128(b"InternalVote");
        let vote_state = history(&vote_prefix);
        let internal_state = history(&internal_prefix);
        assert!(!history(&prefix(b"VotingEngine", b"VoterSnapshot")).is_empty());
        let blocks = finish(12_000);
        let raw = sp_io::storage::get(RESULT).unwrap();
        let counts = Counts::decode_all(&mut raw.as_ref()).unwrap();
        assert_eq!(counts.public, source_public as u64);
        assert_eq!(counts.private, source_private as u64);
        assert_eq!(counts.accounts, source_accounts as u64);
        let mut preserved = 0u64;
        for (public, old, person) in bound_rows {
            let rows = if public {
                publicentity::InstitutionRoleAssignments::<Runtime>::get(
                    &old.cid_number,
                    &old.role_code,
                )
            } else {
                privateentity::InstitutionRoleAssignments::<Runtime>::get(
                    &old.cid_number,
                    &old.role_code,
                )
            };
            assert_eq!(
                rows.iter()
                    .filter(|row| row.person_cid == person
                        && row.cid_number == old.cid_number
                        && row.role_code == old.role_code
                        && row.term_start == old.term_start
                        && row.term_end == old.term_end
                        && row.assignment_source == old.assignment_source
                        && row.assignment_source_ref == old.assignment_source_ref
                        && row.assignment_status == old.assignment_status)
                    .count(),
                1
            );
            preserved += 1;
        }
        assert_eq!(preserved, 37, "本批37个已登记账户各保留原岗位任职");
        assert_eq!(counts.assignments, 49, "37条原岗位任职及程伟12条任职");
        assert_eq!(counts.missing_people, 986);
        assert_eq!(System::block_hash(0), genesis);
        assert_eq!(
            counts.public,
            publicentity::Institutions::<Runtime>::iter().count() as u64
        );
        assert_eq!(
            counts.private,
            privateentity::Institutions::<Runtime>::iter().count() as u64
        );
        assert_eq!(
            counts.accounts,
            (publicentity::InstitutionAccounts::<Runtime>::iter().count()
                + privateentity::InstitutionAccounts::<Runtime>::iter().count()) as u64
        );
        assert_eq!(Balances::total_issuance(), issuance);
        for (wallet, old) in accounts {
            let new = frame_system::Account::<Runtime>::get(wallet);
            assert_eq!(new.nonce, old.nonce);
            assert_eq!(new.data, old.data);
            assert_eq!(new.providers, old.providers);
            assert_eq!(new.consumers, old.consumers);
            assert_eq!(new.sufficients, old.sufficients);
        }
        assert_eq!(history(&vote_prefix), vote_state);
        assert_eq!(history(&internal_prefix), internal_state);
        let current = Identity::person_account(&person).unwrap();
        assert_founder_assignments(&person, &current);
        eprintln!(
            "最终区块 {}，{} 步，汇总 {:?}",
            snapshot["at"], blocks, counts
        );
    });
}

const OLD_GRANDPA: &[u8] = b"GrandpaKeyChange";

fn grandpa_storage(pallet: &[u8]) -> std::collections::BTreeMap<Vec<u8>, Vec<u8>> {
    let prefix = sp_io::hashing::twox_128(pallet);
    let mut cursor = prefix.to_vec();
    let mut entries = std::collections::BTreeMap::new();
    while let Some(key) = sp_io::storage::next_key(&cursor).filter(|key| key.starts_with(&prefix)) {
        entries.insert(key.clone(), sp_io::storage::get(&key).unwrap().to_vec());
        cursor = key;
    }
    entries
}

fn seed_old_grandpa_keys() -> (engingcore::CidNumber, [u8; 32]) {
    let mut result = None;
    for (index, institution) in CHINA_CB.iter().enumerate() {
        let cid = test_cid_number(institution.cid_number.as_bytes());
        let key = sp_core::ed25519::Pair::from_seed(&[(index + 1) as u8; 32])
            .public()
            .0;
        sp_io::storage::set(
            &storage_key(OLD_GRANDPA, b"CurrentGrandpaKeys", &cid),
            &key.encode(),
        );
        sp_io::storage::set(
            &storage_key(OLD_GRANDPA, b"GrandpaKeyOwnerByKey", &key),
            &cid.encode(),
        );
        if index == 0 {
            result = Some((cid, key));
        }
    }
    sp_io::storage::set(
        &prefix(OLD_GRANDPA, b":__STORAGE_VERSION__:"),
        &0u16.encode(),
    );
    result.unwrap()
}

fn assert_grandpa_prefix_moved() {
    let old = grandpa_storage(OLD_GRANDPA);
    assert!(old.len() >= 89);
    let consensus = grandpa_storage(b"Grandpa");
    crate::migrations::GrandpaKeys::on_runtime_upgrade();
    assert!(grandpa_storage(OLD_GRANDPA).is_empty());
    let mut expected: std::collections::BTreeMap<_, _> = old
        .into_iter()
        .map(|(old, value)| {
            let mut key = sp_io::hashing::twox_128(b"GrandpaKey").to_vec();
            key.extend_from_slice(&old[16..]);
            (key, value)
        })
        .collect();
    expected.insert(
        prefix(b"GrandpaKey", b":__STORAGE_VERSION__:").to_vec(),
        1u16.encode(),
    );
    assert_eq!(grandpa_storage(b"GrandpaKey"), expected);
    assert_eq!(grandpa_storage(b"Grandpa"), consensus);
    assert_eq!(
        grandpakey::CurrentGrandpaKeys::<Runtime>::iter().count(),
        CHINA_CB.len(),
    );
    for (cid, key) in grandpakey::CurrentGrandpaKeys::<Runtime>::iter() {
        assert_eq!(
            grandpakey::GrandpaKeyOwnerByKey::<Runtime>::get(key),
            Some(cid)
        );
    }
    let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
    crate::migrations::GrandpaKeys::on_runtime_upgrade();
    assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
}

#[test]
fn grandpa_key_migration_preserves_pending_recovery_nonce_and_consensus() {
    sp_io::TestExternalities::default().execute_with(|| {
        let (cid, key) = seed_old_grandpa_keys();
        let new_key = sp_core::ed25519::Pair::from_seed(&[91; 32]).public().0;
        sp_io::storage::set(
            &storage_key(OLD_GRANDPA, b"ReservedGrandpaKeys", &new_key),
            &cid.encode(),
        );
        sp_io::storage::set(
            &storage_key(OLD_GRANDPA, b"NextGrandpaKeyProofNonce", &cid),
            &9u64.encode(),
        );
        sp_io::storage::set(
            &storage_key(OLD_GRANDPA, b"ActiveEmergencyRecoveryByInstitution", &cid),
            &47u128.encode(),
        );
        let pending = grandpakey::PendingGrandpaKeyChangeState {
            actor_cid_number: cid.clone(),
            initiator_account_id: AccountId::new([99; 32]),
            old_public_key: key,
            new_public_key: new_key,
            proof_nonce: 8,
            change_kind: grandpakey::GrandpaKeyChangeKind::EmergencyRecovery,
            proposal_id: Some(47),
            expected_set_id: 5,
            scheduled_at: 76u32,
            activate_at: 96u32,
        };
        sp_io::storage::set(
            &prefix(OLD_GRANDPA, b"PendingGrandpaKeyChange"),
            &pending.encode(),
        );
        sp_io::storage::set(&prefix(b"Grandpa", b"CurrentSetId"), &4u64.encode());
        let authorized: sp_consensus_grandpa::AuthorityList =
            vec![(sp_core::ed25519::Public::from_raw(key).into(), 1)];
        sp_io::storage::set(&prefix(b"Grandpa", b"Authorities"), &authorized.encode());

        assert_grandpa_prefix_moved();
        assert_eq!(Grandpa::grandpa_authorities(), authorized);
        assert_eq!(Grandpa::current_set_id(), 4);
        assert_eq!(
            grandpakey::NextGrandpaKeyProofNonce::<Runtime>::get(&cid),
            9
        );
        assert_eq!(
            grandpakey::PendingGrandpaKeyChange::<Runtime>::get(),
            Some(pending)
        );
    });
}

#[test]
fn grandpa_key_migration_rejects_target_collision_atomically() {
    sp_io::TestExternalities::default().execute_with(|| {
        let (cid, _) = seed_old_grandpa_keys();
        sp_io::storage::set(
            &storage_key(b"GrandpaKey", b"CurrentGrandpaKeys", &cid),
            &[13; 32],
        );
        let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
        let result = std::panic::catch_unwind(crate::migrations::GrandpaKeys::on_runtime_upgrade);
        assert!(result.is_err());
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
    });
}

#[test]
#[ignore = "通过塔塔控制台显式交付原链最终状态的 GRANDPA 存储副本"]
fn grandpa_key_migration_replays_finalized_authority_state() {
    let path = std::env::var_os("CITIZENCHAIN_GRANDPA_SNAPSHOT").expect("需原链状态副本");
    let state: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(state["genesis"].as_str(), Some(GENESIS));
    let bytes =
        |value: &Value| hex::decode(value.as_str().unwrap().strip_prefix("0x").unwrap()).unwrap();
    sp_io::TestExternalities::default().execute_with(|| {
        for row in state["rows"].as_array().unwrap() {
            sp_io::storage::set(&bytes(&row[0]), &bytes(&row[1]));
        }
        assert_eq!(Grandpa::grandpa_authorities().len(), 1);
        assert_grandpa_prefix_moved();
        assert_eq!(Grandpa::grandpa_authorities().len(), 1);
        eprintln!(
            "GRANDPA 已核验最终区块 {}，44 对公钥索引与共识集合一致",
            state["at"]
        );
    });
}

/// 查看整个外部执行环境的原始状态，包含此次迁移不会触达的上游存储。
fn top_storage() -> std::collections::BTreeMap<Vec<u8>, Vec<u8>> {
    let mut cursor = Vec::new();
    let mut result = std::collections::BTreeMap::new();
    while let Some(key) = sp_io::storage::next_key(&cursor) {
        result.insert(key.clone(), sp_io::storage::get(&key).unwrap().to_vec());
        cursor = key;
    }
    result
}

fn expected_renamed_storage() -> (std::collections::BTreeMap<Vec<u8>, Vec<u8>>, usize) {
    let prefixes = crate::migrations::renamed_pallets().map(|(old, new, _)| {
        (
            sp_io::hashing::twox_128(old.as_bytes()),
            sp_io::hashing::twox_128(new.as_bytes()),
        )
    });
    let mut moved = 0;
    let expected = top_storage()
        .into_iter()
        .map(|(key, value)| {
            let key =
                if let Some((old, new)) = prefixes.iter().find(|(old, _)| key.starts_with(old)) {
                    moved += 1;
                    [new.as_slice(), &key[old.len()..]].concat()
                } else {
                    key
                };
            (key, value)
        })
        .collect();
    (expected, moved)
}

#[test]
fn first_party_prefix_move_keeps_storage_version_and_voting_engine() {
    sp_io::TestExternalities::default().execute_with(|| {
        let renamed = crate::migrations::renamed_pallets();
        assert!(!renamed
            .iter()
            .any(|(old, new, _)| *old == "VotingEngine" || *new == "EngingCore"));
        for (slot, (old, _, _)) in renamed.iter().enumerate() {
            let slot = slot as u32;
            sp_io::storage::set(
                &prefix(old.as_bytes(), b":__STORAGE_VERSION__:"),
                &1u16.encode(),
            );
            sp_io::storage::set(
                &storage_key(old.as_bytes(), b"Fixture", &slot.encode()),
                &slot.encode(),
            );
        }
        sp_io::storage::set(&prefix(b"VotingEngine", b"NextProposalId"), &7u128.encode());
        sp_io::storage::set(&prefix(b"Grandpa", b"CurrentSetId"), &12u64.encode());

        let (expected, moved) = expected_renamed_storage();
        assert_eq!(moved, 34);
        crate::migrations::RenamePallets::on_runtime_upgrade();
        assert_eq!(top_storage(), expected);
        assert_eq!(VotingEngine::next_proposal_id(), 7);
        assert_eq!(Grandpa::current_set_id(), 12);
        crate::migrations::RenamePallets::on_runtime_upgrade();
        assert_eq!(top_storage(), expected);
    });
}

#[test]
fn first_party_prefix_move_rolls_back_all_pallets_on_nonmatching_target_key() {
    sp_io::TestExternalities::default().execute_with(|| {
        sp_io::storage::set(&prefix(b"OnchainTransaction", b"Nonce"), &[11]);
        sp_io::storage::set(&prefix(b"SquarePost", b"Old"), &[22]);
        sp_io::storage::set(&prefix(b"Square", b"DifferentTargetKey"), &[33]);
        let before = top_storage();
        let result = std::panic::catch_unwind(crate::migrations::RenamePallets::on_runtime_upgrade);
        assert!(result.is_err());
        assert_eq!(top_storage(), before);
    });
}

#[test]
fn first_party_prefix_move_rejects_oversized_proof_without_any_write() {
    sp_io::TestExternalities::default().execute_with(|| {
        let budget = crate::configs::RuntimeBlockWeights::get()
            .max_block
            .proof_size()
            / 4;
        sp_io::storage::set(&prefix(b"OnchainTransaction", b"Small"), &[1]);
        let key = prefix(b"SquarePost", b"Large");
        let value = vec![7; (budget / 2) as usize];
        sp_io::storage::set(&key, &value);
        let before = top_storage();
        assert!(
            std::panic::catch_unwind(crate::migrations::RenamePallets::on_runtime_upgrade).is_err()
        );
        assert_eq!(top_storage(), before, "所有来源和目标在预算拒绝后保持原样");
    });
}

#[test]
fn prefix_migrations_accept_only_frame_initialized_version_keys() {
    use frame_support::traits::BeforeAllRuntimeMigrations;

    sp_io::TestExternalities::default().execute_with(|| {
        for (old, _, _) in crate::migrations::renamed_pallets() {
            sp_io::storage::set(
                &prefix(old.as_bytes(), b":__STORAGE_VERSION__:"),
                &0u16.encode(),
            );
            sp_io::storage::set(&prefix(old.as_bytes(), b"Fixture"), &[1, 2, 3]);
        }
        let (cid, _) = seed_old_grandpa_keys();
        // 与 Executive 相同：FRAME 先初始化所有新注册名，再运行本链迁移。
        AllPalletsWithSystem::before_all_runtime_migrations();
        crate::migrations::RenamePallets::on_runtime_upgrade();
        for (old, new, _) in crate::migrations::renamed_pallets() {
            assert!(sp_io::storage::get(&prefix(old.as_bytes(), b"Fixture")).is_none());
            assert_eq!(
                sp_io::storage::get(&prefix(new.as_bytes(), b"Fixture"))
                    .unwrap()
                    .as_ref(),
                &[1, 2, 3]
            );
            // 框架刚写的代码版本不能冒充旧模块的实际存储版本。
            assert_eq!(
                unhashed::get::<u16>(&prefix(new.as_bytes(), b":__STORAGE_VERSION__:")),
                Some(0)
            );
        }
        crate::migrations::GrandpaKeys::on_runtime_upgrade();
        assert!(grandpakey::CurrentGrandpaKeys::<Runtime>::contains_key(cid));
        assert_eq!(StorageVersion::get::<GrandpaKey>(), StorageVersion::new(1));
    });
    for invalid_value in [vec![0xff, 0xff], vec![0]] {
        sp_io::TestExternalities::default().execute_with(|| {
            sp_io::storage::set(&prefix(b"OnchainTransaction", b"Fixture"), &[1]);
            sp_io::storage::set(
                &prefix(b"Onchain", b":__STORAGE_VERSION__:"),
                &invalid_value,
            );
            let before = top_storage();
            assert!(
                std::panic::catch_unwind(crate::migrations::RenamePallets::on_runtime_upgrade)
                    .is_err()
            );
            assert_eq!(top_storage(), before);
        });
    }
}

#[test]
#[ignore = "通过塔塔控制台显式交付现网最终区块的只读前缀快照"]
fn first_party_prefix_move_replays_finalized_state_and_keeps_upstream() {
    let path = std::env::var_os("CITIZENCHAIN_PALLET_SNAPSHOT").expect("需要最终状态副本");
    let snapshot: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(snapshot["genesis"].as_str(), Some(GENESIS));
    let bytes =
        |value: &Value| hex::decode(value.as_str().unwrap().strip_prefix("0x").unwrap()).unwrap();

    sp_io::TestExternalities::default().execute_with(|| {
        for row in snapshot["rows"].as_array().unwrap() {
            sp_io::storage::set(&bytes(&row[0]), &bytes(&row[1]));
        }
        let (expected, moved) = expected_renamed_storage();
        assert!(moved > 0);
        let observed = crate::migrations::RenamePallets::on_runtime_upgrade();
        assert_eq!(top_storage(), expected);
        crate::migrations::RenamePallets::on_runtime_upgrade();
        assert_eq!(top_storage(), expected);
        eprintln!(
            "最终区块 {} 的 {} 个键原地迁移，参考权重 {:?}",
            snapshot["at"], moved, observed
        );
    });
}

/// 必须交付完整最终状态和本轮候选；普通单元测试不连接节点，也不隐式下载候选。
/// 所有业务迁移均由真实 Wasm API 执行，原生环境仅负责迁移前后对账。
#[test]
#[ignore = "通过塔塔控制台显式交付全量最终状态与本轮 Wasm 候选"]
fn full_state_wasm_migration_replay() {
    sp_tracing::try_init_simple();
    use sp_core::traits::{CallContext, RuntimeCode, WrappedRuntimeCode};
    use sp_runtime::{traits::Header as _, StateVersion, Storage};
    use sp_state_machine::{Backend, IterArgs, OverlayedChanges, StateMachine};
    use std::{borrow::Cow, collections::BTreeMap, io::Write, time::Instant};

    type BackendState = sc_client_db::BenchmarkingState<BlakeTwo256>;
    let path = std::env::var_os("CITIZENCHAIN_FULL_SNAPSHOT").expect("需要完整最终状态");
    let snapshot: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(snapshot["genesis"].as_str(), Some(GENESIS));
    assert!(snapshot["children"].as_array().unwrap().is_empty());
    let bytes =
        |value: &Value| hex::decode(value.as_str().unwrap().strip_prefix("0x").unwrap()).unwrap();
    let rows = snapshot["rows"].as_array().unwrap();
    assert!(rows.len() > 500_000, "禁止以机构样本代替完整状态");
    let original: BTreeMap<_, _> = rows
        .iter()
        .map(|row| (bytes(&row[0]), bytes(&row[1])))
        .collect();
    assert_eq!(original.len(), rows.len(), "快照不得包含重复键");
    // 为真实节点的隔离RPC验收交付近期归档；只保存一个完整基点和后续真实状态差量。
    // 末尾完成记录必须在全部业务对账通过后写入，不能将中断现场当作通过的检查点。
    let mut archive = std::env::var_os("CITIZENCHAIN_REPLAY_ARCHIVE").map(|path| {
        let first = std::env::var("CITIZENCHAIN_REPLAY_ARCHIVE_FROM")
            .expect("归档起点必须显式交付")
            .parse::<u32>()
            .unwrap();
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        (first, std::io::BufWriter::new(file))
    });
    let state_version =
        StateVersion::try_from(snapshot["version"]["stateVersion"].as_u64().unwrap() as u8)
            .unwrap();
    // SDK 计量后端的构造器固定采用 V1；从空后端按原链真实 V0 布局导入，
    // 避免重编码长值节点而改变状态根，也不改动锁定 SDK。
    let backend = BackendState::new(Storage::default(), None, true, true).unwrap();
    let (root, transaction) = backend.storage_root(
        original
            .iter()
            .map(|(key, value)| (key.as_slice(), Some(value.as_slice()))),
        state_version,
    );
    backend
        .commit(root, transaction, Vec::new(), Vec::new())
        .unwrap();
    assert_eq!(root.as_bytes(), bytes(&snapshot["header"]["stateRoot"]));

    // 先验证未替换代码时的真实状态根；仅离线后端接收候选，不写真实链。
    let candidate =
        std::fs::read(std::env::var_os("CITIZENCHAIN_REPLAY_WASM").expect("需要本轮 Wasm 候选"))
            .unwrap();
    let fetcher = WrappedRuntimeCode(Cow::Borrowed(&candidate));
    let code = RuntimeCode {
        code_fetcher: &fetcher,
        heap_pages: None,
        hash: sp_io::hashing::blake2_256(&candidate).to_vec(),
    };
    let mut overlay = OverlayedChanges::<BlakeTwo256>::default();
    overlay.set_storage(b":code".to_vec(), Some(candidate.clone()));
    // 单独加入公开合成测试账户，用真实 V5 ML-DSA 备注交易组成非空区块。
    // 原链账户、机构、任职和历史不改；原状态根已在加入夹具前逐字节核对。
    let child = [0x5c; 32];
    let public = mldsa::public_key(&child);
    let signer = AccountId::new(mldsa::account_id(&public));
    let signer_key = frame_system::Account::<Runtime>::hashed_key_for(&signer);
    assert!(!original.contains_key(&signer_key));
    let funding = 1_000_000u128;
    let mut account =
        frame_system::AccountInfo::<Nonce, pallet_balances::AccountData<Balance>>::default();
    account.providers = 1;
    account.data.free = funding;
    overlay.set_storage(signer_key, Some(account.encode()));
    let issuance_key = prefix(b"Balances", b"TotalIssuance").to_vec();
    let expected_issuance =
        u128::decode_all(&mut original[&issuance_key].as_slice()).unwrap() + funding;
    overlay.set_storage(issuance_key.clone(), Some(expected_issuance.encode()));
    let commit = |overlay: &mut OverlayedChanges<BlakeTwo256>| {
        let changes = overlay
            .drain_storage_changes(&backend, state_version)
            .unwrap();
        let proof = backend.proof_size().expect("必须启用真实证明记录");
        let delta = changes.main_storage_changes.clone();
        backend
            .commit(
                changes.transaction_storage_root,
                changes.transaction,
                changes.main_storage_changes,
                changes.child_storage_changes,
            )
            .unwrap();
        (proof, delta)
    };
    commit(&mut overlay);
    let mut executor =
        sc_executor::WasmExecutor::<sp_io::SubstrateHostFunctions>::builder().build();
    let invoke = |executor: &sc_executor::WasmExecutor<sp_io::SubstrateHostFunctions>,
                  overlay: &mut OverlayedChanges<BlakeTwo256>,
                  method: &str,
                  data: &[u8]| {
        StateMachine::new(
            &backend,
            overlay,
            executor,
            method,
            data,
            &mut Default::default(),
            &code,
            CallContext::Offchain,
        )
        .execute()
        .unwrap_or_else(|error| panic!("真实 Wasm API {method} 失败：{error:?}"))
    };
    let version = sp_version::RuntimeVersion::decode_all(
        &mut invoke(&executor, &mut overlay, "Core_version", &[]).as_slice(),
    )
    .unwrap();
    assert_eq!(version.spec_version, 8);
    assert_eq!(version.transaction_version, 7);
    assert_eq!(version.state_version(), state_version);
    assert_eq!(
        invoke(
            &executor,
            &mut overlay,
            "ChainIdentityApi_genesis_hash",
            &[]
        ),
        bytes(&snapshot["genesis"])
    );
    let saved = |prefix: &[u8]| -> BTreeMap<Vec<u8>, Vec<u8>> {
        original
            .iter()
            .filter(|(key, _)| key.starts_with(prefix))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    };
    let source_public = saved(&prefix(b"PublicManage", b"Institutions")).len();
    let source_private = saved(&prefix(b"PrivateManage", b"Institutions")).len();
    let source_accounts = saved(&prefix(b"PublicManage", b"InstitutionAccounts")).len()
        + saved(&prefix(b"PrivateManage", b"InstitutionAccounts")).len();
    assert_eq!(
        (source_public, source_private, source_accounts),
        (49_593, 1, 99_234)
    );
    let system_accounts = saved(&prefix(b"System", b"Account"));
    let mut voting = None;
    let old_snapshots = saved(&prefix(b"VotingEngine", b"VoterSnapshot"))
        .into_values()
        .collect::<Vec<_>>();
    assert!(!old_snapshots.is_empty());
    let grandpa = saved(&sp_io::hashing::twox_128(b"Grandpa"));
    let grandpa_keys = saved(&sp_io::hashing::twox_128(OLD_GRANDPA));
    let timestamp =
        u64::decode_all(&mut original[&prefix(b"Timestamp", b"Now").to_vec()].as_slice()).unwrap();
    let mut parent = Hash::from_slice(&bytes(&snapshot["at"]));
    let first = u32::from_str_radix(
        snapshot["header"]["number"]
            .as_str()
            .unwrap()
            .trim_start_matches("0x"),
        16,
    )
    .unwrap()
        + 1;
    let mut measurements = Vec::new();
    let mut resumed = false;
    let mut completed = false;
    let call = RuntimeCall::System(frame_system::Call::remark {
        remark: b"migration-replay".to_vec(),
    });
    let mut implicit_ext = sp_io::TestExternalities::default();
    let implicit = implicit_ext.execute_with(|| {
        use pallet_revive::evm::runtime::EthExtra;
        use sp_runtime::traits::TransactionExtension;
        frame_system::BlockHash::<Runtime>::insert(
            0,
            Hash::from_slice(&bytes(&snapshot["genesis"])),
        );
        let mut checks: TxExtension = EthExtraImpl::get_eth_extension(0, 0);
        (checks.11).0 = Default::default();
        let mut implicit = TransactionExtension::<RuntimeCall>::implicit(&checks).unwrap();
        implicit.3 = version.spec_version;
        implicit.4 = version.transaction_version;
        implicit
    });
    for step in 0..12_000u32 {
        // 丢弃执行器缓存后仅凭已提交后端继续，检验升级和续跑不会依赖进程内状态。
        if step == 32 {
            executor =
                sc_executor::WasmExecutor::<sp_io::SubstrateHostFunctions>::builder().build();
            resumed = true;
        }
        backend.reset_read_write_count();
        backend.recorder().unwrap().reset();
        let started = Instant::now();
        let header = Header::new(
            first + step,
            Default::default(),
            Default::default(),
            parent,
            Default::default(),
        );
        let mode = sp_runtime::ExtrinsicInclusionMode::decode_all(
            &mut invoke(
                &executor,
                &mut overlay,
                "Core_initialize_block",
                &header.encode(),
            )
            .as_slice(),
        )
        .unwrap();
        let inherent: UncheckedExtrinsic = generic::UncheckedExtrinsic::new_bare(
            RuntimeCall::Timestamp(pallet_timestamp::Call::set {
                now: timestamp + u64::from(step + 1) * 2,
            }),
        )
        .into();
        let applied = sp_runtime::ApplyExtrinsicResult::decode_all(
            &mut invoke(
                &executor,
                &mut overlay,
                "BlockBuilder_apply_extrinsic",
                &inherent.encode(),
            )
            .as_slice(),
        )
        .unwrap();
        assert_eq!(applied, Ok(Ok(())));
        use pallet_revive::evm::runtime::EthExtra;
        let mut checks: TxExtension = EthExtraImpl::get_eth_extension(step, 0);
        (checks.11).0 = Default::default();
        let encoded = mldsa::sign_transaction(
            &child,
            &(call.clone(), checks.clone(), implicit.clone()).encode(),
            step > 0,
            &checks.encode(),
            &call.encode(),
        )
        .unwrap();
        let signed = sp_runtime::ApplyExtrinsicResult::decode_all(
            &mut invoke(
                &executor,
                &mut overlay,
                "BlockBuilder_apply_extrinsic",
                &encoded,
            )
            .as_slice(),
        )
        .unwrap();
        assert_eq!(signed, Ok(Ok(())), "维护块必须接纳真实签名交易");
        let finalized = Header::decode_all(
            &mut invoke(&executor, &mut overlay, "BlockBuilder_finalize_block", &[]).as_slice(),
        )
        .unwrap();
        parent = finalized.hash();
        let (proof, delta) = commit(&mut overlay);
        let maximum = crate::configs::RuntimeBlockWeights::get().max_block;
        assert!(
            u64::from(proof) <= maximum.proof_size() / 4,
            "专用维护块的真实证明必须落在原四分之一预算内：{proof}"
        );
        if let Some((from, writer)) = archive.as_mut() {
            if first + step >= *from {
                let state = if first + step == *from {
                    Some(
                        backend
                            .pairs(IterArgs::default())
                            .unwrap()
                            .map(Result::unwrap)
                            .map(|(key, value)| (hex::encode(key), hex::encode(value)))
                            .collect::<Vec<_>>(),
                    )
                } else {
                    None
                };
                let changes = delta
                    .into_iter()
                    .map(|(key, value)| (hex::encode(key), value.map(hex::encode)))
                    .collect::<Vec<_>>();
                serde_json::to_writer(&mut *writer, &serde_json::json!({
                    "header": hex::encode(finalized.encode()), "state": state, "changes": changes,
                    "body": [hex::encode(inherent.encode()), hex::encode(&encoded)],
                })).unwrap();
                writer.write_all(b"\n").unwrap();
            }
        }
        let counts = backend.read_write_count();
        if step == 0 {
            let vote_prefix = sp_io::hashing::twox_128(b"VotingEngine");
            let mut args = IterArgs::default();
            args.prefix = Some(&vote_prefix);
            voting = Some(
                backend
                    .pairs(args)
                    .unwrap()
                    .map(Result::unwrap)
                    .collect::<BTreeMap<_, _>>(),
            );
        }
        measurements.push(serde_json::json!({
            "block": first + step, "micros": started.elapsed().as_micros(),
            "header": hex::encode(finalized.encode()),
            "proof_bytes": proof, "reads": counts.0, "repeat_reads": counts.1,
            "writes": counts.2, "repeat_writes": counts.3,
        }));
        let mut controls = IterArgs::default();
        controls.prefix = Some(b":citizenchain:");
        let control = backend
            .pairs(controls)
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let mut status = sp_io::TestExternalities::new(Storage {
            top: control,
            ..Default::default()
        });
        completed = status.execute_with(|| {
            !Institutions::pending()
                && !crate::evm_accounts::BackfillEvmAccounts::pending()
                && !sp_io::storage::exists(b":citizenchain:entity-migration:evm-deferred:v1")
        });
        if step % 100 == 0 || completed {
            eprintln!(
                "Wasm 排演区块 {}：模式 {:?}，读写 {:?}，证明 {} 字节",
                first + step,
                mode,
                counts,
                proof
            );
        }
        if completed {
            assert!(matches!(
                mode,
                sp_runtime::ExtrinsicInclusionMode::AllExtrinsics
            ));
            break;
        }
    }
    assert!(
        completed && resumed,
        "完整迁移及 EVM 回填必须有界完成并跨执行器恢复"
    );
    let final_rows: BTreeMap<_, _> = backend
        .pairs(IterArgs::default())
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let final_saved = |prefix: &[u8]| -> BTreeMap<Vec<u8>, Vec<u8>> {
        final_rows
            .iter()
            .filter(|(key, _)| key.starts_with(prefix))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    };
    // 即使后续对账失败也保留真实计量，只有全部断言完成后才标记通过。
    let output = std::env::var_os("CITIZENCHAIN_REPLAY_REPORT").expect("需要计量结果路径");
    std::fs::write(&output, serde_json::to_vec_pretty(&serde_json::json!({
        "passed": false, "at": snapshot["at"], "wasm_hash": hex::encode(&code.hash),
        "spec_version": version.spec_version, "transaction_version": version.transaction_version,
        "source_keys": rows.len(), "resumed": resumed, "synthetic_signed_account": true,
        "blocks": measurements,
    })).unwrap()).unwrap();
    let mut final_accounts = final_saved(&prefix(b"System", b"Account"));
    assert_eq!(final_accounts.len(), system_accounts.len() + 1);
    let synthetic = final_accounts
        .remove(&frame_system::Account::<Runtime>::hashed_key_for(&signer))
        .unwrap();
    let synthetic =
        frame_system::AccountInfo::<Nonce, pallet_balances::AccountData<Balance>>::decode_all(
            &mut synthetic.as_slice(),
        )
        .unwrap();
    assert_eq!(synthetic.nonce, measurements.len() as u32);
    assert_eq!(synthetic.data.free, funding);
    assert!(
        final_accounts == system_accounts,
        "原链账户数据、nonce及引用必须逐字节不变"
    );
    assert_eq!(final_rows[&issuance_key], expected_issuance.encode());
    let mut expected_voting = voting.unwrap();
    let mut history = sp_io::TestExternalities::new(Storage {
        top: expected_voting.clone(),
        ..Default::default()
    });
    let expired = history.execute_with(|| {
        let mut keys = engingcore::ExecutionRetryDeadlines::<Runtime>::iter()
            .filter(|(deadline, _)| *deadline <= first + measurements.len() as u32 - 1)
            .map(|(deadline, ids)| {
                // 正常区块会消费已到期的历史重试桶；只允许预检确认的终态引用，
                // 原提案、票据、计票及21703清理安排仍逐字节对账。
                assert!(ids
                    .iter()
                    .all(|id| engingcore::Proposals::<Runtime>::get(id).is_some_and(
                        |proposal| matches!(
                            proposal.status,
                            engingcore::STATUS_EXECUTED
                                | engingcore::STATUS_REJECTED
                                | engingcore::STATUS_EXECUTION_FAILED
                        )
                    )));
                engingcore::ExecutionRetryDeadlines::<Runtime>::hashed_key_for(deadline)
            })
            .collect::<Vec<_>>();
        for (deadline, ids) in engingcore::ProposalsByExpiry::<Runtime>::iter() {
            if deadline <= first + measurements.len() as u32 - 1 {
                assert!(ids
                    .iter()
                    .all(|id| engingcore::Proposals::<Runtime>::get(id).is_some_and(
                        |proposal| matches!(
                            proposal.status,
                            engingcore::STATUS_EXECUTED
                                | engingcore::STATUS_REJECTED
                                | engingcore::STATUS_EXECUTION_FAILED
                        )
                    )));
                keys.push(engingcore::ProposalsByExpiry::<Runtime>::hashed_key_for(
                    deadline,
                ));
            }
        }
        keys
    });
    for key in &expired {
        assert!(expected_voting.remove(key).is_some());
    }
    assert!(
        final_saved(&sp_io::hashing::twox_128(b"VotingEngine")) == expected_voting,
        "历史提案、票据、计票和清理安排必须不变；仅消费已到期终态期限索引"
    );
    assert_eq!(
        final_saved(&prefix(b"VotingEngine", b"VoterSnapshot"))
            .into_values()
            .collect::<Vec<_>>(),
        old_snapshots
    );
    assert_eq!(final_saved(&sp_io::hashing::twox_128(b"Grandpa")), grandpa);
    let mut moved_grandpa: BTreeMap<_, _> = grandpa_keys
        .into_iter()
        .map(|(key, value)| {
            (
                [
                    sp_io::hashing::twox_128(b"GrandpaKey").as_slice(),
                    &key[16..],
                ]
                .concat(),
                value,
            )
        })
        .collect();
    moved_grandpa.insert(
        prefix(b"GrandpaKey", b":__STORAGE_VERSION__:").to_vec(),
        StorageVersion::new(1).encode(),
    );
    assert_eq!(
        final_saved(&sp_io::hashing::twox_128(b"GrandpaKey")),
        moved_grandpa
    );
    let mut final_ext = sp_io::TestExternalities::new(Storage {
        top: final_rows.clone(),
        ..Default::default()
    });
    final_ext.execute_with(|| {
        let counts = unhashed::get::<Counts>(RESULT).unwrap();
        assert_eq!(counts.public, source_public as u64);
        assert_eq!(counts.private, source_private as u64);
        assert_eq!(counts.accounts, source_accounts as u64);
        assert_eq!(counts.assignments, 49);
        assert_eq!(counts.missing_people, 986);
        assert_eq!(
            publicentity::Institutions::<Runtime>::iter().count(),
            source_public
        );
        assert_eq!(
            privateentity::Institutions::<Runtime>::iter().count(),
            source_private
        );
        assert_eq!(
            publicentity::InstitutionAccounts::<Runtime>::iter().count()
                + privateentity::InstitutionAccounts::<Runtime>::iter().count(),
            source_accounts
        );
        let person = test_cid_number(
            primitives::cid::china::citizenchain::LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER
                .as_bytes(),
        );
        let current = Identity::person_account(&person).unwrap();
        assert_founder_assignments(&person, &current);
        assert_eq!(
            identity::BindingRevisionByCid::<Runtime>::get(&person),
            Some(2)
        );
        assert_eq!(Grandpa::grandpa_authorities().len(), 1);
        assert_eq!(System::account_nonce(&signer), measurements.len() as u32);
        assert_eq!(Balances::free_balance(&signer), funding);
        assert!(unhashed::get::<bool>(b":citizenchain:evm-account-mapping:v1").unwrap());
        use pallet_revive::{AccountId32Mapper, AddressMapper};
        let mut mapped = std::collections::BTreeSet::new();
        mapped.extend(
            frame_system::Account::<Runtime>::iter_keys().filter(|account| account != &signer),
        );
        mapped.extend(publicentity::AccountRegisteredCid::<Runtime>::iter_keys());
        mapped.extend(privateentity::AccountRegisteredCid::<Runtime>::iter_keys());
        mapped.extend(identity::CidByAccountId::<Runtime>::iter_keys());
        assert!(mapped.len() >= source_accounts);
        for account in &mapped {
            if !AccountId32Mapper::<Runtime>::is_eth_derived(account) {
                assert!(
                    AccountId32Mapper::<Runtime>::is_mapped(account),
                    "原链登记账户缺少EVM映射"
                );
                assert_eq!(
                    AccountId32Mapper::<Runtime>::to_account_id(
                        &AccountId32Mapper::<Runtime>::to_address(account)
                    ),
                    *account
                );
            }
        }
        eprintln!("已逐项核验 {} 个原链账户的 EVM 映射", mapped.len());
        eprintln!("完整 Wasm 对账通过：{:?}", counts);
    });
    if let Some(path) = std::env::var_os("CITIZENCHAIN_REPLAY_STATE") {
        // 只给后续隔离RPC测试交付已完成全部对账的公开状态；不是主链升级或共识证明。
        let rows = final_rows
            .iter()
            .map(|(key, value)| {
                [
                    format!("0x{}", hex::encode(key)),
                    format!("0x{}", hex::encode(value)),
                ]
            })
            .collect::<Vec<_>>();
        std::fs::write(
            path,
            serde_json::to_vec(&serde_json::json!({
                "genesis": snapshot["genesis"], "source_at": snapshot["at"],
                "last_hash": format!("0x{}", hex::encode(parent.as_bytes())),
                "rows": rows, "synthetic_signed_account": true,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    if let Some((from, mut writer)) = archive {
        assert!(
            measurements.last().unwrap()["block"].as_u64().unwrap() >= u64::from(from) + 256,
            "真实RPC近期窗口必须具有全部执行父状态"
        );
        serde_json::to_writer(
            &mut writer,
            &serde_json::json!({
                "complete": true, "genesis": snapshot["genesis"],
                "last_hash": format!("0x{}", hex::encode(parent.as_bytes())),
                "wasm_hash": hex::encode(&code.hash),
            }),
        )
        .unwrap();
        writer.write_all(b"\n").unwrap();
        writer.flush().unwrap();
    }
    std::fs::write(
        output,
        serde_json::to_vec_pretty(&serde_json::json!({
        "passed": true, "at": snapshot["at"], "wasm_hash": hex::encode(&code.hash), "spec_version": version.spec_version,
            "transaction_version": version.transaction_version, "source_keys": rows.len(),
            "resumed": resumed, "synthetic_signed_account": true, "blocks": measurements,
        }))
        .unwrap(),
    )
    .unwrap();
}

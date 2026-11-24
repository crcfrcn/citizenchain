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

/// 初始化真实 Runtime 区块时，应由 FRAME 暂停普通交易而继续接受固有交易。
#[test]
fn entity_migration_uses_frame_inherent_only_mode() {
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
            sp_runtime::ExtrinsicInclusionMode::OnlyInherents,
        );
        assert!(Institutions::pending());
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
            vec![primitives::governance_skeleton::province_commissioner_role_code(*b"GZ")],
            false,
            3,
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
    assert_eq!((public.len(), private.len()), (4, 3));
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
        2
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
fn entity_migration_keeps_five_rebound_assignments_and_adds_two_seats_once() {
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
        assert_eq!(unhashed::get::<Counts>(RESULT).unwrap().assignments, 7);
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
fn entity_migration_founder_target_conflict_does_not_write_either_new_seat() {
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
#[ignore = "通过塔塔控制台显式传入最终区块的只读状态副本"]
fn entity_migration_replays_finalized_chain_state() {
    let path = std::env::var_os("CITIZENCHAIN_ENTITY_SNAPSHOT").expect("需要最终状态副本");
    let snapshot: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(snapshot["genesis"].as_str(), Some(GENESIS));
    let rows = snapshot["rows"].as_array().unwrap();
    sp_io::TestExternalities::default().execute_with(|| {
        let bytes = |value: &Value| {
            hex::decode(value.as_str().unwrap().strip_prefix("0x").unwrap()).unwrap()
        };
        for row in rows {
            sp_io::storage::set(&bytes(&row[0]), &bytes(&row[1]));
        }
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
    let prefixes = crate::migrations::renamed_pallets().map(|(old, new)| {
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
            .any(|(old, new)| *old == "VotingEngine" || *new == "EngingCore"));
        for (slot, (old, _)) in renamed.iter().enumerate() {
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

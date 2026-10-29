//! 复用本模块真实测试环境的行为与边界回归。
use super::*;

#[test]
fn register_voting_identity_stores_identity_and_counts_scope() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("0001");

        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("0001")),
            identity_version(&citizen_cid_number("0001")),
            identity_expires_at(),
            valid_signature(),
        ));

        assert!(VotingIdentityByCid::<Test>::contains_key(cid(
            &citizen_cid_number("0001")
        )));
        assert_eq!(
            AccountIdByCid::<Test>::get(cid(&citizen_cid_number("0001"))),
            Some(1)
        );
        assert_eq!(
            CidByAccountId::<Test>::get(1),
            Some(cid(&citizen_cid_number("0001")))
        );
        assert_eq!(CountryVotingCount::<Test>::get(), 1);
        assert_eq!(ProvinceVotingCount::<Test>::get(code(b"43")), 1);
        assert!(CitizenIdentity::voting_subject(&1, &town_scope()).is_some());
    });
}

#[test]
fn natp_resident_cannot_become_voting_citizen() {
    new_test_ext().execute_with(|| {
        // q3 补充约束:NATP 居民永远无法升级投票/竞选公民(register 走 CTZN 校验)。
        let natp = primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: "natp-civic",
                p1: "1",
                province_code: "",
                province_name: "",
                city_code: "",
                city_name: "",
                year: "2026",
                institution: "NATP",
            },
        )
        .expect("natp cid should generate")
        .into_bytes();
        assert_ok!(CitizenIdentity::self_occupy_cid(
            RuntimeOrigin::signed(5),
            cid(&natp),
        ));
        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(5, &natp),
                identity_version(&natp),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::InvalidCitizenCode
        );
    });
}

#[test]
fn candidate_identity_requires_full_profile_and_enables_candidate_reader() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("CANDIDATE");

        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            candidate_payload(1, &citizen_cid_number("CANDIDATE")),
            identity_version(&citizen_cid_number("CANDIDATE")),
            identity_expires_at(),
            valid_signature(),
        ));

        assert!(CandidateIdentityByCid::<Test>::contains_key(cid(
            &citizen_cid_number("CANDIDATE")
        )));
        assert!(CitizenIdentity::voting_subject(&1, &town_scope()).is_some());
        assert!(CitizenIdentity::candidate_subject(&1, &town_scope()).is_some());
    });
}

#[test]
fn candidate_identity_requires_family_name_and_given_name_separately() {
    new_test_ext().execute_with(|| {
        occupy_tag_as("EMPTY-FAMILY", 1);
        let mut empty_family = candidate_payload(1, &citizen_cid_number("EMPTY-FAMILY"));
        empty_family.family_name = Default::default();
        assert_noop!(
            CitizenIdentity::upgrade_to_candidate_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                empty_family.clone(),
                candidate_version(&empty_family),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::EmptyFamilyName
        );

        occupy_tag_as("EMPTY-GIVEN", 2);
        let mut empty_given = candidate_payload(2, &citizen_cid_number("EMPTY-GIVEN"));
        empty_given.given_name = Default::default();
        assert_noop!(
            CitizenIdentity::upgrade_to_candidate_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                empty_given.clone(),
                candidate_version(&empty_given),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::EmptyGivenName
        );
    });
}

#[test]
fn revoke_identity_marks_status_and_removes_effective_population() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("REVOKE");

        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            candidate_payload(1, &citizen_cid_number("REVOKE")),
            identity_version(&citizen_cid_number("REVOKE")),
            identity_expires_at(),
            valid_signature(),
        ));
        assert_ok!(CitizenIdentity::revoke_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            cid(&citizen_cid_number("REVOKE")),
        ));

        let stored = VotingIdentityByCid::<Test>::get(cid(&citizen_cid_number("REVOKE")))
            .expect("identity should remain");
        assert_eq!(stored.citizen_status, CitizenStatus::Revoked);
        assert!(!CandidateIdentityByCid::<Test>::contains_key(cid(
            &citizen_cid_number("REVOKE")
        )));
        assert_eq!(
            BindingRevisionByCid::<Test>::get(cid(&citizen_cid_number("REVOKE"))),
            Some(2)
        );
        assert_eq!(CountryVotingCount::<Test>::get(), 0);
        assert!(CitizenIdentity::voting_subject(&1, &town_scope()).is_none());

        let cid_number = cid(&citizen_cid_number("REVOKE"));
        let identity_before = VotingIdentityByCid::<Test>::get(&cid_number);
        let record_before = CidRegistry::<Test>::get(&cid_number);
        let revision_before = BindingRevisionByCid::<Test>::get(&cid_number);
        assert_noop!(
            CitizenIdentity::revoke_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                cid_number.clone(),
            ),
            Error::<Test>::CidAlreadyRevoked
        );
        assert_eq!(
            VotingIdentityByCid::<Test>::get(&cid_number),
            identity_before
        );
        assert_eq!(CidRegistry::<Test>::get(&cid_number), record_before);
        assert_eq!(
            BindingRevisionByCid::<Test>::get(&cid_number),
            revision_before
        );
        assert_eq!(CountryVotingCount::<Test>::get(), 0);
    });
}

#[test]
fn population_data_reads_current_scope_count() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("0001");

        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("0001")),
            identity_version(&citizen_cid_number("0001")),
            identity_expires_at(),
            valid_signature(),
        ));

        let population_data = CitizenIdentity::governance_population_data(&town_scope())
            .expect("current population should be ready");
        assert_eq!(population_data.eligible_total, 1);
        assert_eq!(population_data.eligibility_revision, 1);
        assert_eq!(population_data.eligibility_date, 20260702);
        let voter_subject =
            CitizenIdentity::voting_subject_at_population_data(&1, &population_data)
                .expect("snapshot eligibility should return the complete citizen subject");
        assert_eq!(voter_subject.cid_number, citizen_cid_number("0001"));
        assert_eq!(voter_subject.account_id, 1);
    });
}

#[test]
fn population_data_revision_freezes_membership_before_identity_update() {
    new_test_ext().execute_with(|| {
        occupy_tag("SNAPSHOT-OLD");
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("SNAPSHOT-OLD")),
            identity_version(&citizen_cid_number("SNAPSHOT-OLD")),
            identity_expires_at(),
            valid_signature(),
        ));

        let old_population_data = CitizenIdentity::governance_population_data(&town_scope())
            .expect("old population should be ready");
        assert_eq!(old_population_data.eligible_total, 1);
        assert!(
            CitizenIdentity::voting_subject_at_population_data(&1, &old_population_data).is_some()
        );

        // 同一账户迁往另一乡镇后，旧提案仍按创建时身份判断；新提案使用新身份。
        let mut moved = voting_payload(1, &citizen_cid_number("SNAPSHOT-OLD"));
        moved.residence_town_code = code(b"4301002");
        assert_ok!(CitizenIdentity::update_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            moved.clone(),
            voting_version(&moved),
            identity_expires_at(),
            valid_signature(),
        ));

        assert!(
            CitizenIdentity::voting_subject_at_population_data(&1, &old_population_data).is_some()
        );
        let new_population_data = CitizenIdentity::governance_population_data(&town_scope())
            .expect("new population should be ready");
        assert_eq!(new_population_data.eligible_total, 0);
        assert!(
            CitizenIdentity::voting_subject_at_population_data(&1, &new_population_data).is_none()
        );
    });
}

#[test]
fn expired_passport_cannot_vote_and_is_excluded_from_population() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("EXPIRED");

        let mut payload = voting_payload(1, &citizen_cid_number("EXPIRED"));
        payload.passport_valid_from = 20200101;
        payload.passport_valid_until = 20250101;

        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            payload.clone(),
            voting_version(&payload),
            identity_expires_at(),
            valid_signature(),
        ));

        assert_eq!(CountryVotingCount::<Test>::get(), 0);
        assert!(CitizenIdentity::voting_subject(&1, &town_scope()).is_none());
        assert!(CitizenIdentity::candidate_subject(&1, &town_scope()).is_none());
    });
}

#[test]
fn not_yet_valid_passport_cannot_vote() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("FUTURE");

        let mut payload = voting_payload(1, &citizen_cid_number("FUTURE"));
        payload.passport_valid_from = 20300101;
        payload.passport_valid_until = 20400101;

        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            payload.clone(),
            voting_version(&payload),
            identity_expires_at(),
            valid_signature(),
        ));

        assert_eq!(CountryVotingCount::<Test>::get(), 0);
        assert_eq!(PopulationTransitionCountByDate::<Test>::get(20300101), 1);
        assert!(CitizenIdentity::voting_subject(&1, &town_scope()).is_none());
    });
}

#[test]
fn first_population_date_must_initialize_before_identity_write() {
    new_test_ext().execute_with(|| {
        PopulationReadyDate::<Test>::kill();
        occupy_tag("BOOTSTRAP");
        assert_noop!(
            CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                voting_payload(1, &citizen_cid_number("BOOTSTRAP")),
                identity_version(&citizen_cid_number("BOOTSTRAP")),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::PopulationDataNotReady
        );

        CitizenIdentity::on_idle(System::block_number(), frame_support::weights::Weight::MAX);
        assert_eq!(PopulationReadyDate::<Test>::get(), 20260702);
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            voting_payload(1, &citizen_cid_number("BOOTSTRAP")),
            identity_version(&citizen_cid_number("BOOTSTRAP")),
            identity_expires_at(),
            valid_signature(),
        ));
    });
}

#[test]
fn passport_activates_on_valid_from_and_deactivates_after_valid_until() {
    new_test_ext().execute_with(|| {
        occupy_tag("ONE-DAY");
        let mut payload = voting_payload(1, &citizen_cid_number("ONE-DAY"));
        payload.passport_valid_from = 20260703;
        payload.passport_valid_until = 20260703;
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            payload.clone(),
            voting_version(&payload),
            identity_expires_at(),
            valid_signature(),
        ));
        assert_eq!(CountryVotingCount::<Test>::get(), 0);

        set_day_offset(1);
        assert!(CitizenIdentity::governance_population_data(&PopulationScope::Country).is_none());
        CitizenIdentity::on_idle(System::block_number(), frame_support::weights::Weight::MAX);
        assert_eq!(PopulationReadyDate::<Test>::get(), 20260703);
        assert_eq!(CountryVotingCount::<Test>::get(), 1);
        assert_eq!(
            CitizenIdentity::governance_population_data(&PopulationScope::Country)
                .expect("activation date should be ready")
                .eligibility_date,
            20260703
        );

        set_day_offset(2);
        CitizenIdentity::on_idle(System::block_number(), frame_support::weights::Weight::MAX);
        assert_eq!(PopulationReadyDate::<Test>::get(), 20260704);
        assert_eq!(CountryVotingCount::<Test>::get(), 0);
    });
}

#[test]
fn population_transition_limit_hides_partial_day_and_blocks_identity_changes() {
    new_test_ext().execute_with(|| {
        for (account_id, tag) in [(1, "BATCH-1"), (2, "BATCH-2"), (3, "BATCH-3")] {
            occupy_tag_as(tag, account_id);
            let mut payload = voting_payload(account_id, &citizen_cid_number(tag));
            payload.passport_valid_from = 20260703;
            payload.passport_valid_until = 20300101;
            assert_ok!(CitizenIdentity::register_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                payload.clone(),
                voting_version(&payload),
                identity_expires_at(),
                valid_signature(),
            ));
        }

        set_day_offset(1);
        CitizenIdentity::on_idle(System::block_number(), frame_support::weights::Weight::MAX);
        assert_eq!(PopulationReadyDate::<Test>::get(), 20260702);
        assert_eq!(PopulationTransitionCursorByDate::<Test>::get(20260703), 2);
        assert_eq!(CountryVotingCount::<Test>::get(), 2);
        assert!(CitizenIdentity::governance_population_data(&PopulationScope::Country).is_none());

        let mut update = voting_payload(1, &citizen_cid_number("BATCH-1"));
        update.passport_valid_from = 20260703;
        update.passport_valid_until = 20310101;
        assert_noop!(
            CitizenIdentity::update_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                update.clone(),
                voting_version(&update),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::PopulationDataNotReady
        );

        CitizenIdentity::on_idle(System::block_number(), frame_support::weights::Weight::MAX);
        assert_eq!(PopulationReadyDate::<Test>::get(), 20260703);
        assert_eq!(CountryVotingCount::<Test>::get(), 3);
        assert!(CitizenIdentity::governance_population_data(&PopulationScope::Country).is_some());
    });
}

#[test]
fn identity_update_invalidates_old_population_transitions_by_revision() {
    new_test_ext().execute_with(|| {
        occupy_tag("RESCHEDULE");
        let mut first = voting_payload(1, &citizen_cid_number("RESCHEDULE"));
        first.passport_valid_from = 20260703;
        first.passport_valid_until = 20260703;
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            first.clone(),
            voting_version(&first),
            identity_expires_at(),
            valid_signature(),
        ));

        let mut replacement = voting_payload(1, &citizen_cid_number("RESCHEDULE"));
        replacement.passport_valid_from = 20260704;
        replacement.passport_valid_until = 20260705;
        assert_ok!(CitizenIdentity::update_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            replacement.clone(),
            voting_version(&replacement),
            identity_expires_at(),
            valid_signature(),
        ));

        set_day_offset(1);
        CitizenIdentity::on_idle(System::block_number(), frame_support::weights::Weight::MAX);
        assert_eq!(CountryVotingCount::<Test>::get(), 0);
        set_day_offset(2);
        CitizenIdentity::on_idle(System::block_number(), frame_support::weights::Weight::MAX);
        assert_eq!(PopulationReadyDate::<Test>::get(), 20260704);
        assert_eq!(CountryVotingCount::<Test>::get(), 1);
    });
}

#[test]
fn population_faults_closed_when_chain_date_moves_backwards() {
    new_test_ext().execute_with(|| {
        set_day_offset(-1);
        CitizenIdentity::on_idle(System::block_number(), frame_support::weights::Weight::MAX);
        assert_eq!(
            PopulationMaintenanceFault::<Test>::get(),
            Some(PopulationFault::DateMovedBackwards)
        );
        assert!(CitizenIdentity::governance_population_data(&PopulationScope::Country).is_none());
    });
}

#[test]
fn candidate_identity_stores_sex_and_public_profile() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("SEX");

        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            candidate_payload(1, &citizen_cid_number("SEX")),
            identity_version(&citizen_cid_number("SEX")),
            identity_expires_at(),
            valid_signature(),
        ));

        let stored = CandidateIdentityByCid::<Test>::get(cid(&citizen_cid_number("SEX")))
            .expect("candidate stored");
        assert_eq!(stored.citizen_sex, CitizenSex::Female);
        assert_eq!(stored.family_name, family_name(b"Citizen"));
        assert_eq!(stored.given_name, given_name(b"One"));
        assert_eq!(stored.birth_date, 20000131);
        // 固定链上日 20260702 − 出生 20000131 → 26 周岁。
        assert_eq!(CitizenIdentity::candidate_age(&1), Some(26));
    });
}

#[test]
fn candidate_birth_date_is_immutable_on_update() {
    new_test_ext().execute_with(|| {
        occupy_tag("IMMUT");
        let cid = citizen_cid_number("IMMUT");
        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            candidate_payload(1, &cid),
            identity_version(&cid),
            identity_expires_at(),
            valid_signature(),
        ));

        // 更新竞选身份时试图改出生日期 → 拒绝。
        let mut tampered = candidate_payload(1, &cid);
        tampered.birth_date = 19990101;
        assert_noop!(
            CitizenIdentity::update_candidate_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                tampered.clone(),
                candidate_version(&tampered),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::BirthDateImmutable
        );
    });
}

#[test]
fn candidate_birth_scope_and_sex_are_immutable_on_update() {
    new_test_ext().execute_with(|| {
        occupy_tag("IMMUT-PROFILE");
        let cid = citizen_cid_number("IMMUT-PROFILE");
        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            candidate_payload(1, &cid),
            identity_version(&cid),
            identity_expires_at(),
            valid_signature(),
        ));

        // 更新竞选身份时试图改性别 → 拒绝(D4a:填后锁定)。
        let mut tampered_sex = candidate_payload(1, &cid);
        tampered_sex.citizen_sex = CitizenSex::Male;
        assert_noop!(
            CitizenIdentity::update_candidate_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                tampered_sex.clone(),
                candidate_version(&tampered_sex),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::CandidateProfileImmutable
        );

        // 试图改出生市码 → 同样拒绝。
        let mut tampered_city = candidate_payload(1, &cid);
        tampered_city.birth_city_code = code(b"4302");
        assert_noop!(
            CitizenIdentity::update_candidate_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                tampered_city.clone(),
                candidate_version(&tampered_city),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::CandidateProfileImmutable
        );
    });
}

#[test]
fn candidate_illegal_birth_date_rejected() {
    new_test_ext().execute_with(|| {
        occupy_tag("BADDOB");
        let mut payload = candidate_payload(1, &citizen_cid_number("BADDOB"));
        payload.birth_date = 20261340; // 非法月/日
        assert_noop!(
            CitizenIdentity::upgrade_to_candidate_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                payload.clone(),
                candidate_version(&payload),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::InvalidBirthDate
        );
    });
}

#[test]
fn candidate_future_birth_date_rejected() {
    new_test_ext().execute_with(|| {
        occupy_tag("FUTDOB");
        let mut payload = candidate_payload(1, &citizen_cid_number("FUTDOB"));
        payload.birth_date = 20990101; // 未来出生 → 算不出年龄
        assert_noop!(
            CitizenIdentity::upgrade_to_candidate_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                payload.clone(),
                candidate_version(&payload),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::InvalidBirthDate
        );
    });
}

#[test]
fn candidate_under_sixteen_cannot_register_onchain_identity() {
    new_test_ext().execute_with(|| {
        // 占号先行:身份写入前置。
        occupy_tag("UNDERAGE");

        // 投票身份不再链上校验年龄;最小年龄门只在竞选身份按 birth_date 实时判定。
        // 固定时间 20260702 下出生 20150301 → 11 周岁,未满 16。
        let mut payload = candidate_payload(1, &citizen_cid_number("UNDERAGE"));
        payload.birth_date = 20150301;

        assert_noop!(
            CitizenIdentity::upgrade_to_candidate_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                payload.clone(),
                candidate_version(&payload),
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::UnderCandidateAge
        );
    });
}

/// 历史载荷重放被拒：迁居换照后，旧签名不得把居住地与护照窗口回滚。
#[test]
fn replaying_an_old_voting_payload_is_rejected_by_identity_version() {
    new_test_ext().execute_with(|| {
        occupy_tag("REPLAY-OLD");
        let cid_bytes = citizen_cid_number("REPLAY-OLD");
        let original = voting_payload(1, &cid_bytes);
        let original_version = identity_version(&cid_bytes);
        assert_ok!(CitizenIdentity::register_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            original.clone(),
            original_version,
            identity_expires_at(),
            valid_signature(),
        ));

        // 公民迁往另一乡镇并换发护照，有效期延长。
        let mut moved = voting_payload(1, &cid_bytes);
        moved.residence_town_code = code(b"4301002");
        moved.passport_valid_until = 20400630;
        assert_ok!(CitizenIdentity::update_voting_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            moved,
            identity_version(&cid_bytes),
            identity_expires_at(),
            valid_signature(),
        ));

        // 原载荷 + 原版本号原样重提：版本已推进，拒绝。
        assert_noop!(
            CitizenIdentity::update_voting_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                original,
                original_version,
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::IdentityVersionMismatch
        );

        // 链上仍是迁居后的状态，未被回滚。
        let current = VotingIdentityByCid::<Test>::get(cid(&cid_bytes)).expect("identity");
        assert_eq!(current.residence_town_code.as_slice(), b"4301002");
        assert_eq!(current.passport_valid_until, 20400630);
    });
}

/// 竞选身份写入同样受版本保护：旧版本号提交即拒。
#[test]
fn replaying_an_old_candidate_payload_is_rejected_by_identity_version() {
    new_test_ext().execute_with(|| {
        occupy_tag("REPLAY-CAND");
        let cid_bytes = citizen_cid_number("REPLAY-CAND");
        let payload = candidate_payload(1, &cid_bytes);
        let stale_version = identity_version(&cid_bytes);
        assert_ok!(CitizenIdentity::upgrade_to_candidate_identity(
            RuntimeOrigin::signed(100),
            registrar_cid_number(),
            registrar_role_code(),
            payload.clone(),
            stale_version,
            identity_expires_at(),
            valid_signature(),
        ));

        assert_noop!(
            CitizenIdentity::update_candidate_identity(
                RuntimeOrigin::signed(100),
                registrar_cid_number(),
                registrar_role_code(),
                payload,
                stale_version,
                identity_expires_at(),
                valid_signature(),
            ),
            Error::<Test>::IdentityVersionMismatch
        );
    });
}

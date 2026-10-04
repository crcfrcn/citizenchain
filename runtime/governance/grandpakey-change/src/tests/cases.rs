#![cfg(test)]

use super::*;

const PROOF_EXPIRES_AT: u64 = 20;

fn proof_payload(
    node_index: usize,
    who: AccountId32,
    new_public_key: [u8; 32],
    proof_nonce: u64,
    change_kind: GrandpaKeyChangeKind,
) -> GrandpaKeyProofPayload<AccountId32, u64, sp_core::H256> {
    crate::proof::payload::<Test>(
        cb_cid(node_index),
        committee_role(),
        who,
        CurrentGrandpaKeys::<Test>::get(cb_cid(node_index)).expect("test key exists"),
        new_public_key,
        proof_nonce,
        PROOF_EXPIRES_AT,
        change_kind,
    )
}

fn sign_proof(
    pair: &ed25519::Pair,
    payload: &GrandpaKeyProofPayload<AccountId32, u64, sp_core::H256>,
) -> [u8; 64] {
    pair.sign(&crate::proof::signing_digest(payload)).0
}

fn propose_emergency(
    node_index: usize,
    proposer_index: usize,
    new_pair: &ed25519::Pair,
) -> DispatchResult {
    let who = cb_admin(node_index, proposer_index);
    let payload = proof_payload(
        node_index,
        who.clone(),
        new_pair.public().0,
        0,
        GrandpaKeyChangeKind::EmergencyRecovery,
    );
    GrandpaKeyChange::propose_emergency_grandpa_key_recovery(
        RuntimeOrigin::signed(who),
        cb_cid(node_index),
        committee_role(),
        new_pair.public().0,
        0,
        PROOF_EXPIRES_AT,
        sign_proof(new_pair, &payload),
    )
}

fn schedule_routine(
    node_index: usize,
    proposer_index: usize,
    new_pair: &ed25519::Pair,
    proof_nonce: u64,
) -> DispatchResult {
    let who = cb_admin(node_index, proposer_index);
    let payload = proof_payload(
        node_index,
        who.clone(),
        new_pair.public().0,
        proof_nonce,
        GrandpaKeyChangeKind::RoutineRotation,
    );
    GrandpaKeyChange::schedule_grandpa_key_rotation(
        RuntimeOrigin::signed(who),
        cb_cid(node_index),
        committee_role(),
        new_pair.public().0,
        proof_nonce,
        PROOF_EXPIRES_AT,
        sign_proof(&grandpa_pair(node_index), &payload),
        sign_proof(new_pair, &payload),
    )
}

#[test]
fn weak_small_order_new_key_is_rejected_before_signature_check() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            GrandpaKeyChange::propose_emergency_grandpa_key_recovery(
                RuntimeOrigin::signed(prc_admin(0)),
                prc_cid(),
                committee_role(),
                identity_public_key(),
                0,
                PROOF_EXPIRES_AT,
                [0u8; 64],
            ),
            Error::<Test>::InvalidEd25519Key
        );
    });
}

#[test]
fn emergency_recovery_is_only_the_target_institutions_internal_vote() {
    new_test_ext().execute_with(|| {
        let new_pair = key_pair(31);
        assert_ok!(propose_emergency(1, 0, &new_pair));
        let proposal_id = last_proposal_id();
        let proposal = VotingEngine::proposals(proposal_id).expect("proposal exists");
        let plan = VotingEngine::proposal_vote_plan(proposal_id).expect("vote plan exists");

        assert_eq!(proposal.kind, PROPOSAL_KIND_INTERNAL);
        assert_eq!(proposal.internal_code, Some(PRC));
        assert_eq!(proposal.actor_cid_number, Some(prc_cid()));
        assert_eq!(plan.voting_engine, VotingEngineKind::Internal);
        assert_eq!(plan.voter_subjects.len(), 1);
        assert!(matches!(
            &plan.voter_subjects[0],
            AuthorizationSubject::Institution(subject)
                if subject.cid_number == prc_cid() && subject.role_code == committee_role()
        ));
        assert!(Grandpa::pending_change().is_none());
    });
}

#[test]
fn emergency_recovery_schedules_after_internal_threshold_and_updates_only_after_activation() {
    new_test_ext().execute_with(|| {
        let actor_cid_number = prc_cid();
        let old_public_key =
            CurrentGrandpaKeys::<Test>::get(actor_cid_number.clone()).expect("old key exists");
        let new_pair = key_pair(32);
        let new_public_key = new_pair.public().0;

        assert_ok!(propose_emergency(1, 0, &new_pair));
        let proposal_id = last_proposal_id();
        pass_prc_proposal(1, proposal_id);

        assert!(Grandpa::pending_change().is_some());
        assert_eq!(
            CurrentGrandpaKeys::<Test>::get(actor_cid_number.clone()),
            Some(old_public_key)
        );
        assert_eq!(
            ReservedGrandpaKeys::<Test>::get(new_public_key),
            Some(actor_cid_number.clone())
        );

        finalize_grandpa_at(1 + GrandpaChangeDelay::get());

        assert_eq!(
            CurrentGrandpaKeys::<Test>::get(actor_cid_number.clone()),
            Some(new_public_key)
        );
        assert!(GrandpaKeyOwnerByKey::<Test>::get(old_public_key).is_none());
        assert_eq!(
            GrandpaKeyOwnerByKey::<Test>::get(new_public_key),
            Some(actor_cid_number)
        );
        assert!(PendingGrandpaKeyChange::<Test>::get().is_none());
        assert!(ReservedGrandpaKeys::<Test>::get(new_public_key).is_none());
        assert!(System::events().iter().any(|record| matches!(
            &record.event,
            RuntimeEvent::GrandpaKeyChange(Event::<Test>::GrandpaKeyActivated {
                change_kind: GrandpaKeyChangeKind::EmergencyRecovery,
                ..
            })
        )));
    });
}

#[test]
fn rejected_emergency_recovery_releases_active_state_and_reserved_key() {
    new_test_ext().execute_with(|| {
        let new_pair = key_pair(33);
        assert_ok!(propose_emergency(1, 0, &new_pair));
        let proposal_id = last_proposal_id();

        // 发起人自动计入反对票之外，再补足足够反对票使提案终态否决。
        for admin_index in 1..5 {
            assert_ok!(cast_vote(cb_admin(1, admin_index), proposal_id, false));
        }
        <VotingEngine as Hooks<u64>>::on_initialize(System::block_number());

        assert!(ActiveEmergencyRecoveryByInstitution::<Test>::get(prc_cid()).is_none());
        assert!(ReservedGrandpaKeys::<Test>::get(new_pair.public().0).is_none());
        assert!(Grandpa::pending_change().is_none());
    });
}

#[test]
fn routine_rotation_uses_call_one_without_creating_a_vote() {
    new_test_ext().execute_with(|| {
        let old_public_key = CurrentGrandpaKeys::<Test>::get(prc_cid()).expect("old key exists");
        let new_pair = key_pair(34);
        assert_ok!(schedule_routine(1, 0, &new_pair, 0));

        assert_eq!(VotingEngine::next_proposal_id(), 0);
        assert!(Grandpa::pending_change().is_some());
        assert_eq!(
            CurrentGrandpaKeys::<Test>::get(prc_cid()),
            Some(old_public_key)
        );
        assert_eq!(NextGrandpaKeyProofNonce::<Test>::get(prc_cid()), 1);

        finalize_grandpa_at(1 + GrandpaChangeDelay::get());
        assert_eq!(
            CurrentGrandpaKeys::<Test>::get(prc_cid()),
            Some(new_pair.public().0)
        );
        assert!(System::events().iter().any(|record| matches!(
            &record.event,
            RuntimeEvent::GrandpaKeyChange(Event::<Test>::GrandpaKeyActivated {
                change_kind: GrandpaKeyChangeKind::RoutineRotation,
                ..
            })
        )));
    });
}

#[test]
fn routine_rotation_requires_both_old_and_new_private_key_signatures() {
    new_test_ext().execute_with(|| {
        let who = prc_admin(0);
        let new_pair = key_pair(35);
        let payload = proof_payload(
            1,
            who.clone(),
            new_pair.public().0,
            0,
            GrandpaKeyChangeKind::RoutineRotation,
        );

        assert_noop!(
            GrandpaKeyChange::schedule_grandpa_key_rotation(
                RuntimeOrigin::signed(who.clone()),
                prc_cid(),
                committee_role(),
                new_pair.public().0,
                0,
                PROOF_EXPIRES_AT,
                [0u8; 64],
                sign_proof(&new_pair, &payload),
            ),
            Error::<Test>::InvalidOldKeySignature
        );
        assert_noop!(
            GrandpaKeyChange::schedule_grandpa_key_rotation(
                RuntimeOrigin::signed(who),
                prc_cid(),
                committee_role(),
                new_pair.public().0,
                0,
                PROOF_EXPIRES_AT,
                sign_proof(&grandpa_pair(1), &payload),
                [0u8; 64],
            ),
            Error::<Test>::InvalidNewKeySignature
        );
    });
}

#[test]
fn proof_nonce_and_expiry_prevent_replay() {
    new_test_ext().execute_with(|| {
        let new_pair = key_pair(36);
        assert_ok!(schedule_routine(1, 0, &new_pair, 0));
        finalize_grandpa_at(1 + GrandpaChangeDelay::get());

        let second_pair = key_pair(37);
        assert_noop!(
            schedule_routine(1, 0, &second_pair, 0),
            Error::<Test>::InvalidProofNonce
        );

        System::set_block_number(PROOF_EXPIRES_AT + 1);
        let who = prc_admin(0);
        let payload = proof_payload(
            1,
            who.clone(),
            second_pair.public().0,
            1,
            GrandpaKeyChangeKind::EmergencyRecovery,
        );
        assert_noop!(
            GrandpaKeyChange::propose_emergency_grandpa_key_recovery(
                RuntimeOrigin::signed(who),
                prc_cid(),
                committee_role(),
                second_pair.public().0,
                1,
                PROOF_EXPIRES_AT,
                sign_proof(&second_pair, &payload),
            ),
            Error::<Test>::ProofExpired
        );
    });
}

#[test]
fn routine_rotation_rejects_non_member_and_wrong_institution() {
    new_test_ext().execute_with(|| {
        let new_pair = key_pair(38);
        let outsider = AccountId32::new([99u8; 32]);
        let payload = proof_payload(
            1,
            outsider.clone(),
            new_pair.public().0,
            0,
            GrandpaKeyChangeKind::RoutineRotation,
        );
        assert_noop!(
            GrandpaKeyChange::schedule_grandpa_key_rotation(
                RuntimeOrigin::signed(outsider),
                prc_cid(),
                committee_role(),
                new_pair.public().0,
                0,
                PROOF_EXPIRES_AT,
                sign_proof(&grandpa_pair(1), &payload),
                sign_proof(&new_pair, &payload),
            ),
            Error::<Test>::UnauthorizedAdmin
        );

        let invalid_cid: CidNumber = b"invalid-cid".to_vec().try_into().unwrap();
        assert_noop!(
            GrandpaKeyChange::propose_emergency_grandpa_key_recovery(
                RuntimeOrigin::signed(prc_admin(0)),
                invalid_cid,
                committee_role(),
                new_pair.public().0,
                0,
                PROOF_EXPIRES_AT,
                [0u8; 64],
            ),
            Error::<Test>::InvalidInstitution
        );
    });
}

#[test]
fn emergency_recovery_blocks_routine_rotation_for_same_institution() {
    new_test_ext().execute_with(|| {
        let recovery_pair = key_pair(39);
        assert_ok!(propose_emergency(1, 0, &recovery_pair));

        let routine_pair = key_pair(40);
        assert_noop!(
            schedule_routine(1, 0, &routine_pair, 1),
            Error::<Test>::EmergencyRecoveryAlreadyActive
        );
    });
}

#[test]
fn second_change_waits_until_the_first_authority_change_activates() {
    new_test_ext().execute_with(|| {
        let first_pair = key_pair(41);
        assert_ok!(schedule_routine(1, 0, &first_pair, 0));

        let second_pair = key_pair(42);
        assert_noop!(
            schedule_routine(2, 0, &second_pair, 0),
            Error::<Test>::GrandpaChangePending
        );
    });
}

#[test]
fn key_owned_by_another_institution_cannot_be_reused() {
    new_test_ext().execute_with(|| {
        let who = prc_admin(0);
        let other_key = grandpa_public_key(0);
        assert_noop!(
            GrandpaKeyChange::propose_emergency_grandpa_key_recovery(
                RuntimeOrigin::signed(who),
                prc_cid(),
                committee_role(),
                other_key,
                0,
                PROOF_EXPIRES_AT,
                [0u8; 64],
            ),
            Error::<Test>::NewKeyAlreadyUsed
        );
    });
}

#[test]
fn emergency_execution_remains_retryable_while_another_grandpa_change_is_pending() {
    new_test_ext().execute_with(|| {
        let new_pair = key_pair(43);
        assert_ok!(propose_emergency(1, 0, &new_pair));
        let proposal_id = last_proposal_id();
        assert_ok!(Grandpa::schedule_change(
            grandpa_authorities(),
            GrandpaChangeDelay::get(),
            None,
        ));

        pass_prc_proposal(1, proposal_id);
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal remains retryable")
                .status,
            STATUS_PASSED
        );
        assert_eq!(
            ReservedGrandpaKeys::<Test>::get(new_pair.public().0),
            Some(prc_cid())
        );

        finalize_grandpa_at(1 + GrandpaChangeDelay::get());
        assert_ok!(VotingEngine::retry_passed_proposal(
            RuntimeOrigin::signed(prc_admin(0)),
            proposal_id,
        ));
        assert!(Grandpa::pending_change().is_some());
    });
}

#[test]
fn non_active_catalog_key_cannot_be_rotated_or_recovered() {
    new_test_ext().execute_with(|| {
        // 测试 authority set 只启用前三把；目录中其余 PRC 公钥不能假装成活跃 authority。
        let node_index = 3;
        let new_pair = key_pair(44);
        let who = cb_admin(node_index, 0);
        let payload = crate::proof::payload::<Test>(
            cb_cid(node_index),
            committee_role(),
            who.clone(),
            CurrentGrandpaKeys::<Test>::get(cb_cid(node_index)).expect("catalog key exists"),
            new_pair.public().0,
            0,
            PROOF_EXPIRES_AT,
            GrandpaKeyChangeKind::EmergencyRecovery,
        );
        assert_noop!(
            GrandpaKeyChange::propose_emergency_grandpa_key_recovery(
                RuntimeOrigin::signed(who),
                cb_cid(node_index),
                committee_role(),
                new_pair.public().0,
                0,
                PROOF_EXPIRES_AT,
                sign_proof(&new_pair, &payload),
            ),
            Error::<Test>::OldAuthorityNotFound
        );
    });
}

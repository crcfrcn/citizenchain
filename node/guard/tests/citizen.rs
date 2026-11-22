//! 验证首次身份奖励的精确发行及奖励档位和数量边界。

use super::*;
use codec::Encode;
type State = BTreeMap<Vec<u8>, Vec<u8>>;
fn fixture(
    count: u64,
) -> (
    State,
    State,
    State,
    BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    Vec<u8>,
    [u8; 32],
) {
    let cid = b"GD001-CTZN1-REWARD".to_vec();
    let a = [7; 32];
    let parent = State::from([(storage_key::rewarded_count(), count.encode())]);
    let mut pre = parent.clone();
    pre.insert(
        storage_key::voting_identity(&cid),
        (
            20260101u32,
            20360101u32,
            CitizenStatus::Normal,
            b"GD".to_vec(),
            b"001".to_vec(),
            Vec::<u8>::new(),
            1u32,
        )
            .encode(),
    );
    pre.insert(storage_key::account_id_by_cid(&cid), a.encode());
    pre.insert(storage_key::cid_by_account_id(&a), cid.encode());
    let mut post = pre.clone();
    post.insert(storage_key::rewarded_count(), (count + 1).encode());
    post.insert(storage_key::identity_claimed(&cid), ().encode());
    post.insert(storage_key::account_rewarded(&a), ().encode());
    let delta = BTreeMap::from([
        (storage_key::rewarded_count(), Some((count + 1).encode())),
        (storage_key::identity_claimed(&cid), Some(().encode())),
        (storage_key::account_rewarded(&a), Some(().encode())),
    ]);
    (parent, pre, post, delta, cid, a)
}
fn run(
    parent: &State,
    pre: &State,
    post: &State,
    d: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
) -> Result<FinalizeIssuancePlan, GuardError> {
    let mut plan = FinalizeIssuancePlan::default();
    check_transition(
        d,
        &|k| parent.get(k).cloned(),
        &|k| pre.get(k).cloned(),
        &|k| post.get(k).cloned(),
        &mut plan,
    )?;
    Ok(plan)
}
#[test]
fn first_identity_facts_produce_the_exact_reward() {
    for (count, amount) in [
        (0, CITIZEN_ISSUANCE_HIGH_REWARD),
        (
            CITIZEN_ISSUANCE_HIGH_REWARD_COUNT,
            CITIZEN_ISSUANCE_NORMAL_REWARD,
        ),
    ] {
        let (p, pre, post, d, _, _) = fixture(count);
        assert_eq!(run(&p, &pre, &post, &d).unwrap().total, amount);
    }
}
#[test]
fn reward_tier_boundary_and_count_limit_are_exact() {
    assert_eq!(
        reward_amount_at(CITIZEN_ISSUANCE_HIGH_REWARD_COUNT - 1),
        CITIZEN_ISSUANCE_HIGH_REWARD
    );
    assert_eq!(
        reward_amount_at(CITIZEN_ISSUANCE_HIGH_REWARD_COUNT),
        CITIZEN_ISSUANCE_NORMAL_REWARD
    );
    let (p, pre, post, d, _, _) = fixture(CITIZEN_ISSUANCE_MAX_COUNT);
    assert_eq!(
        run(&p, &pre, &post, &d),
        Err(GuardError::RewardLimitExceeded)
    );
}
#[test]
fn existing_identity_or_previous_reward_cannot_claim_again() {
    let (mut p, pre, post, d, cid, a) = fixture(0);
    p.insert(
        storage_key::voting_identity(&cid),
        pre[&storage_key::voting_identity(&cid)].clone(),
    );
    assert_eq!(
        run(&p, &pre, &post, &d),
        Err(GuardError::IdentityAlreadyExisted)
    );
    p.remove(&storage_key::voting_identity(&cid));
    p.insert(storage_key::account_rewarded(&a), ().encode());
    assert_eq!(
        run(&p, &pre, &post, &d),
        Err(GuardError::RewardAlreadyClaimed)
    );
}
#[test]
fn reward_identity_indices_and_result_counts_must_match() {
    let (p, mut pre, mut post, mut d, cid, a) = fixture(0);
    pre.insert(
        storage_key::cid_by_account_id(&a),
        b"other".to_vec().encode(),
    );
    assert_eq!(
        run(&p, &pre, &post, &d),
        Err(GuardError::IdentityReverseIndexMismatch)
    );
    pre.insert(storage_key::cid_by_account_id(&a), cid.encode());
    post.insert(storage_key::rewarded_count(), 2u64.encode());
    assert_eq!(
        run(&p, &pre, &post, &d),
        Err(GuardError::RewardedCountInvalid)
    );
    post.insert(storage_key::rewarded_count(), 1u64.encode());
    post.remove(&storage_key::account_rewarded(&a));
    d.remove(&storage_key::account_rewarded(&a));
    assert_eq!(
        run(&p, &pre, &post, &d),
        Err(GuardError::RewardedCountInvalid)
    );
}
#[test]
fn permanent_reward_facts_cannot_be_erased() {
    let (mut p, pre, mut post, mut d, cid, _) = fixture(0);
    let k = storage_key::identity_claimed(&cid);
    p.insert(k.clone(), ().encode());
    post.remove(&k);
    d.insert(k, None);
    assert_eq!(
        run(&p, &pre, &post, &d),
        Err(GuardError::RewardAlreadyClaimed)
    );
}
#[test]
fn genesis_has_no_permanent_rewards() {
    check_genesis_key_values(State::new().iter()).unwrap();
    let s = State::from([(storage_key::rewarded_count(), 1u64.encode())]);
    assert!(check_genesis_key_values(s.iter()).is_err());
}

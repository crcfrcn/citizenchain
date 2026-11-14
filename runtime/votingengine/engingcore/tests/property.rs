//! 随机账户、动作与对象哈希不得绕开个人多签投票计划的主体约束。
use crate::types::*;
use frame_support::{traits::ConstU32, BoundedVec};
use proptest::prelude::*;

type Plan = VotePlan<[u8; 32], ConstU32<32>>;
fn tag() -> BoundedVec<u8, ConstU32<32>> {
    b"multisig".to_vec().try_into().unwrap()
}
proptest! {
    #![proptest_config(ProptestConfig {cases: 128, failure_persistence: None, rng_seed: proptest::test_runner::RngSeed::Fixed(20261029), ..ProptestConfig::default()})]
    #[test]
    fn personal_vote_plan_never_accepts_a_different_voter(account in any::<[u8;32]>(), other in any::<[u8;32]>(), action in any::<u32>(), hash in any::<[u8;32]>()) {
        prop_assume!(account!=other);
        let business=BusinessActionId {module_tag:tag(),action_code:action};
        let subject=AuthorizationSubject::PersonalMultisig(account);
        let good=Plan::try_new(business.clone(),tag(),subject.clone(),vec![subject.clone()],VotingEngineKind::Internal,hash);
        prop_assert!(good.is_ok());
        let bad=Plan::try_new(business,tag(),subject,vec![AuthorizationSubject::PersonalMultisig(other)],VotingEngineKind::Internal,hash);
        prop_assert!(matches!(bad,Err(VotePlanValidationError::AuthorizationSubjectMismatch)));
    }
}

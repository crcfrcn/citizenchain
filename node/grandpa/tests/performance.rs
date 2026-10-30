use super::*;
use sp_keyring::Ed25519Keyring;
use substrate_test_runtime_client::{
    runtime::Block, DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
};

#[test]
#[ignore = "第5步固定机器显式voter快照恢复预算"]
fn voter_snapshot_restore_fixed_workload_budget() {
    let budget: u128 = std::env::var("CITIZENCHAIN_GRANDPA_PERF_MS")
        .expect("缺少voter恢复预算")
        .parse()
        .expect("预算必须是正整数毫秒");
    assert!(budget > 0);
    let client = TestClientBuilder::new().build();
    let authorities =
        crate::authorities::AuthoritySet::genesis(vec![(Ed25519Keyring::Alice.public().into(), 1)])
            .unwrap();
    let state = VoterSetState::<Block>::live(0, &authorities, (Default::default(), 0));
    write_voter_set_state(&client, &state).unwrap();
    let start = std::time::Instant::now();
    for _ in 0..10_000 {
        std::hint::black_box(
            load_decode::<_, VoterSetState<Block>>(&client, SET_STATE_KEY)
                .unwrap()
                .unwrap(),
        );
    }
    assert!(
        start.elapsed().as_millis() <= budget,
        "voter快照恢复固定工作量超过预算"
    );
}

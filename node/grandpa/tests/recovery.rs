use super::*;
use sp_keyring::Ed25519Keyring;
use substrate_test_runtime_client::{
    runtime::Block, DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
};

#[test]
fn voter_state_roundtrip_overwrites_latest_snapshot_and_rejects_corruption() {
    let client = TestClientBuilder::new().build();
    let authorities =
        crate::authorities::AuthoritySet::genesis(vec![(Ed25519Keyring::Alice.public().into(), 1)])
            .unwrap();
    let live = VoterSetState::<Block>::live(0, &authorities, (Default::default(), 0));
    write_voter_set_state(&client, &live).unwrap();
    let restored = load_decode::<_, VoterSetState<Block>>(&client, SET_STATE_KEY)
        .unwrap()
        .unwrap();
    assert_eq!(restored.encode(), live.encode());
    let paused = VoterSetState::Paused {
        completed_rounds: live.completed_rounds(),
    };
    write_voter_set_state(&client, &paused).unwrap();
    let restored = load_decode::<_, VoterSetState<Block>>(&client, SET_STATE_KEY)
        .unwrap()
        .unwrap();
    assert_eq!(restored.encode(), paused.encode());
    client
        .insert_aux(&[(SET_STATE_KEY, [255u8].as_slice())], &[])
        .unwrap();
    assert!(load_decode::<_, VoterSetState<Block>>(&client, SET_STATE_KEY).is_err());
}

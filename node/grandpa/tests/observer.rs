use super::*;

use crate::{
    aux_schema,
    communication::tests::{make_test_network, Event},
};
use assert_matches::assert_matches;
use sc_network_types::PeerId;
use sc_utils::mpsc::tracing_unbounded;
use sp_blockchain::HeaderBackend as _;
use substrate_test_runtime_client::{TestClientBuilder, TestClientBuilderExt};

use futures::executor;

/// Ensure `Future` implementation of `ObserverWork` is polling its `NetworkBridge`.
/// Regression test for bug introduced in d4fbb897c and fixed in b7af8b339.
///
/// When polled, `NetworkBridge` forwards reputation change requests from the
/// `GossipValidator` to the underlying `dyn Network`. This test triggers a reputation change
/// by calling `GossipValidator::validate` with an invalid gossip message. After polling the
/// `ObserverWork` which should poll the `NetworkBridge`, the reputation change should be
/// forwarded to the test network.
#[test]
fn observer_work_polls_underlying_network_bridge() {
    // Create a test network.
    let (tester_fut, _network) = make_test_network();
    let mut tester = executor::block_on(tester_fut);

    // Create an observer.
    let (client, backend) = {
        let builder = TestClientBuilder::with_default_backend();
        let backend = builder.backend();
        let (client, _) = builder.build_with_longest_chain();
        (Arc::new(client), backend)
    };

    let voters = vec![(sp_keyring::Ed25519Keyring::Alice.public().into(), 1)];

    let persistent_data =
        aux_schema::load_persistent(&*backend, client.info().genesis_hash, 0, || Ok(voters))
            .unwrap();

    let (_tx, voter_command_rx) = tracing_unbounded("test_mpsc_voter_command", 100_000);

    let observer = ObserverWork::new(
        client,
        tester.net_handle.clone(),
        persistent_data,
        None,
        voter_command_rx,
        None,
        None,
    );

    // Trigger a reputation change through the gossip validator.
    let peer_id = PeerId::random();
    tester.trigger_gossip_validator_reputation_change(&peer_id);

    executor::block_on(async move {
        // Poll the observer once and have it forward the reputation change from the gossip
        // validator to the test network.
        assert!(observer.now_or_never().is_none());

        assert_matches!(
            tester.events.next().now_or_never(),
            Some(Some(Event::Report(_, _)))
        );
    });
}

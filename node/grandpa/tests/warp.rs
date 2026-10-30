use super::WarpSyncProof;
use crate::{AuthoritySetChanges, GrandpaJustification};
use codec::Encode;
use rand::prelude::*;
use sc_block_builder::BlockBuilderBuilder;
use sp_blockchain::HeaderBackend;
use sp_consensus::BlockOrigin;
use sp_consensus_grandpa::GRANDPA_ENGINE_ID;
use sp_keyring::Ed25519Keyring;
use std::sync::Arc;
use substrate_test_runtime_client::{
    BlockBuilderExt, ClientBlockImportExt, ClientExt, DefaultTestClientBuilderExt,
    TestClientBuilder, TestClientBuilderExt,
};

#[test]
fn warp_sync_proof_generate_verify() {
    let mut rng = rand::rngs::StdRng::from_seed([0; 32]);
    let builder = TestClientBuilder::new();
    let backend = builder.backend();
    let client = Arc::new(builder.build());

    let available_authorities = Ed25519Keyring::iter().collect::<Vec<_>>();
    let genesis_authorities = vec![(Ed25519Keyring::Alice.public().into(), 1)];

    let mut current_authorities = vec![Ed25519Keyring::Alice];
    let mut current_set_id = 0;
    let mut authority_set_changes = Vec::new();

    for n in 1..=100 {
        let mut builder = BlockBuilderBuilder::new(&*client)
            .on_parent_block(client.chain_info().best_hash)
            .with_parent_block_number(client.chain_info().best_number)
            .build()
            .unwrap();
        let mut new_authorities = None;

        // we will trigger an authority set change every 10 blocks
        if n != 0 && n % 10 == 0 {
            // pick next authorities and add digest for the set change
            let n_authorities = rng.gen_range(1..available_authorities.len());
            let next_authorities = available_authorities
                .choose_multiple(&mut rng, n_authorities)
                .cloned()
                .collect::<Vec<_>>();

            new_authorities = Some(next_authorities.clone());

            let next_authorities = next_authorities
                .iter()
                .map(|keyring| (keyring.public().into(), 1))
                .collect::<Vec<_>>();

            let digest = sp_runtime::generic::DigestItem::Consensus(
                sp_consensus_grandpa::GRANDPA_ENGINE_ID,
                sp_consensus_grandpa::ConsensusLog::ScheduledChange(
                    sp_consensus_grandpa::ScheduledChange {
                        delay: 0u64,
                        next_authorities,
                    },
                )
                .encode(),
            );

            builder.push_deposit_log_digest_item(digest).unwrap();
        }

        let block = builder.build().unwrap().block;

        futures::executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();

        if let Some(new_authorities) = new_authorities {
            // generate a justification for this block, finalize it and note the authority set
            // change
            let (target_hash, target_number) = {
                let info = client.info();
                (info.best_hash, info.best_number)
            };

            let mut precommits = Vec::new();
            for keyring in &current_authorities {
                let precommit = finality_grandpa::Precommit {
                    target_hash,
                    target_number,
                };

                let msg = finality_grandpa::Message::Precommit(precommit.clone());
                let encoded = sp_consensus_grandpa::localized_payload(42, current_set_id, &msg);
                let signature = keyring.sign(&encoded[..]).into();

                let precommit = finality_grandpa::SignedPrecommit {
                    precommit,
                    signature,
                    id: keyring.public().into(),
                };

                precommits.push(precommit);
            }

            let commit = finality_grandpa::Commit {
                target_hash,
                target_number,
                precommits,
            };

            let justification = GrandpaJustification::from_commit(&client, 42, commit).unwrap();

            client
                .finalize_block(
                    target_hash,
                    Some((GRANDPA_ENGINE_ID, justification.encode())),
                )
                .unwrap();

            authority_set_changes.push((current_set_id, n));

            current_set_id += 1;
            current_authorities = new_authorities;
        }
    }

    let authority_set_changes = AuthoritySetChanges::from(authority_set_changes);

    // generate a warp sync proof
    let genesis_hash = client.hash(0).unwrap().unwrap();

    let warp_sync_proof =
        WarpSyncProof::generate(&*backend, genesis_hash, &authority_set_changes).unwrap();

    // verifying the proof should yield the last set id and authorities
    let (new_set_id, new_authorities) = warp_sync_proof
        .verify(0, genesis_authorities, &Default::default())
        .unwrap();

    let expected_authorities = current_authorities
        .iter()
        .map(|keyring| (keyring.public().into(), 1))
        .collect::<Vec<_>>();

    assert_eq!(new_set_id, current_set_id);
    assert_eq!(new_authorities, expected_authorities);
}

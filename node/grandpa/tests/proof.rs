use super::*;
use crate::{authorities::AuthoritySetChanges, BlockNumberOps, ClientError, SetId};
use futures::executor::block_on;
use sc_block_builder::BlockBuilderBuilder;
use sc_client_api::{apply_aux, LockImportRun};
use sp_consensus::BlockOrigin;
use sp_consensus_grandpa::GRANDPA_ENGINE_ID as ID;
use sp_core::crypto::UncheckedFrom;
use sp_keyring::Ed25519Keyring;
use substrate_test_runtime_client::{
    runtime::{Block, Header, H256},
    Backend as TestBackend, ClientBlockImportExt, ClientExt, DefaultTestClientBuilderExt,
    TestClient, TestClientBuilder, TestClientBuilderExt,
};

/// Check GRANDPA proof-of-finality for the given block.
///
/// Returns the vector of headers that MUST be validated + imported
/// AND if at least one of those headers is invalid, all other MUST be considered invalid.
fn check_finality_proof<Block: BlockT>(
    current_set_id: SetId,
    current_authorities: sp_consensus_grandpa::AuthorityList,
    remote_proof: Vec<u8>,
) -> sp_blockchain::Result<super::FinalityProof<Block::Header>>
where
    NumberFor<Block>: BlockNumberOps,
{
    let proof = super::FinalityProof::<Block::Header>::decode(&mut &remote_proof[..])
        .map_err(|_| ClientError::BadJustification("failed to decode finality proof".into()))?;

    let justification: GrandpaJustification<Block> = Decode::decode(&mut &proof.justification[..])
        .map_err(|_| ClientError::JustificationDecode)?;

    justification.verify(current_set_id, &current_authorities)?;

    Ok(proof)
}

pub(crate) type FinalityProof = super::FinalityProof<Header>;

fn header(number: u64) -> Header {
    let parent_hash = match number {
        0 => Default::default(),
        _ => header(number - 1).hash(),
    };
    Header::new(
        number,
        H256::from_low_u64_be(0),
        H256::from_low_u64_be(0),
        parent_hash,
        Default::default(),
    )
}

fn test_blockchain(
    number_of_blocks: u64,
    to_finalize: &[u64],
) -> (Arc<TestClient>, Arc<TestBackend>, Vec<Block>) {
    let builder = TestClientBuilder::new();
    let backend = builder.backend();
    let client = Arc::new(builder.build());

    let mut blocks = Vec::new();
    for _ in 0..number_of_blocks {
        let block = BlockBuilderBuilder::new(&*client)
            .on_parent_block(client.chain_info().best_hash)
            .with_parent_block_number(client.chain_info().best_number)
            .build()
            .unwrap()
            .build()
            .unwrap()
            .block;
        block_on(client.import(BlockOrigin::Own, block.clone())).unwrap();
        blocks.push(block);
    }

    for block in to_finalize {
        let hash = blocks[*block as usize - 1].hash();
        client.finalize_block(hash, None).unwrap();
    }
    (client, backend, blocks)
}

fn store_best_justification(client: &TestClient, just: &GrandpaJustification<Block>) {
    client
        .lock_import_and_run(|import_op| {
            crate::aux_schema::update_best_justification(just, |insert| {
                apply_aux(import_op, insert, &[])
            })
        })
        .unwrap();
}

#[test]
fn finality_proof_fails_if_no_more_last_finalized_blocks() {
    let (_, backend, _) = test_blockchain(6, &[4]);
    let authority_set_changes = AuthoritySetChanges::empty();

    // The last finalized block is 4, so we cannot provide further justifications.
    let proof_of_5 = prove_finality(&*backend, authority_set_changes, 5, true);
    assert!(matches!(
        proof_of_5,
        Err(FinalityProofError::BlockNotYetFinalized)
    ));
}

#[test]
fn finality_proof_is_none_if_no_justification_known() {
    let (_, backend, _) = test_blockchain(6, &[4]);

    let mut authority_set_changes = AuthoritySetChanges::empty();
    authority_set_changes.append(0, 4);

    // Block 4 is finalized without justification
    // => we can't prove finality of 3
    let proof_of_3 = prove_finality(&*backend, authority_set_changes, 3, true).unwrap();
    assert_eq!(proof_of_3, None);
}

#[test]
fn finality_proof_check_fails_when_proof_decode_fails() {
    // When we can't decode proof from Vec<u8>
    check_finality_proof::<Block>(
        1,
        vec![(UncheckedFrom::unchecked_from([3u8; 32]), 1u64)],
        vec![42],
    )
    .unwrap_err();
}

#[test]
fn finality_proof_check_fails_when_proof_is_empty() {
    // When decoded proof has zero length
    check_finality_proof::<Block>(
        1,
        vec![(UncheckedFrom::unchecked_from([3u8; 32]), 1u64)],
        Vec::<GrandpaJustification<Block>>::new().encode(),
    )
    .unwrap_err();
}

#[test]
fn finality_proof_check_fails_with_incomplete_justification() {
    let (_, _, blocks) = test_blockchain(8, &[4, 5, 8]);

    // Create a commit without precommits
    let commit = finality_grandpa::Commit {
        target_hash: blocks[7].hash(),
        target_number: *blocks[7].header().number(),
        precommits: Vec::new(),
    };

    let grandpa_just: GrandpaJustification<Block> =
        sp_consensus_grandpa::GrandpaJustification::<Header> {
            round: 8,
            votes_ancestries: Vec::new(),
            commit,
        }
        .into();

    let finality_proof = FinalityProof {
        block: header(2).hash(),
        justification: grandpa_just.encode(),
        unknown_headers: Vec::new(),
    };

    check_finality_proof::<Block>(
        1,
        vec![(UncheckedFrom::unchecked_from([3u8; 32]), 1u64)],
        finality_proof.encode(),
    )
    .unwrap_err();
}

fn create_commit<S, Id>(
    block: Block,
    round: u64,
    set_id: SetId,
    auth: &[Ed25519Keyring],
) -> finality_grandpa::Commit<H256, u64, S, Id>
where
    Id: From<sp_core::ed25519::Public>,
    S: From<sp_core::ed25519::Signature>,
{
    let mut precommits = Vec::new();

    for voter in auth {
        let precommit = finality_grandpa::Precommit {
            target_hash: block.hash(),
            target_number: *block.header().number(),
        };

        let msg = finality_grandpa::Message::Precommit(precommit.clone());
        let encoded = sp_consensus_grandpa::localized_payload(round, set_id, &msg);
        let signature = voter.sign(&encoded[..]).into();

        let signed_precommit = finality_grandpa::SignedPrecommit {
            precommit,
            signature,
            id: voter.public().into(),
        };
        precommits.push(signed_precommit);
    }

    finality_grandpa::Commit {
        target_hash: block.hash(),
        target_number: *block.header().number(),
        precommits,
    }
}

#[test]
fn finality_proof_check_works_with_correct_justification() {
    let (client, _, blocks) = test_blockchain(8, &[4, 5, 8]);

    let alice = Ed25519Keyring::Alice;
    let set_id = 1;
    let round = 8;
    let commit = create_commit(blocks[7].clone(), round, set_id, &[alice]);
    let grandpa_just = GrandpaJustification::from_commit(&client, round, commit).unwrap();

    let finality_proof = FinalityProof {
        block: header(2).hash(),
        justification: grandpa_just.encode(),
        unknown_headers: Vec::new(),
    };
    assert_eq!(
        finality_proof,
        check_finality_proof::<Block>(
            set_id,
            vec![(alice.public().into(), 1u64)],
            finality_proof.encode(),
        )
        .unwrap(),
    );
}

#[test]
fn finality_proof_using_authority_set_changes_fails_with_undefined_start() {
    let (_, backend, _) = test_blockchain(8, &[4, 5, 8]);

    // We have stored the correct block number for the relevant set, but as we are missing the
    // block for the preceding set the start is not well-defined.
    let mut authority_set_changes = AuthoritySetChanges::empty();
    authority_set_changes.append(1, 8);

    let proof_of_6 = prove_finality(&*backend, authority_set_changes, 6, true);
    assert!(matches!(
        proof_of_6,
        Err(FinalityProofError::BlockNotInAuthoritySetChanges)
    ));
}

#[test]
fn finality_proof_using_authority_set_changes_works() {
    let (client, backend, blocks) = test_blockchain(8, &[4, 5]);
    let block7 = &blocks[6];
    let block8 = &blocks[7];

    let round = 8;
    let commit = create_commit(block8.clone(), round, 1, &[Ed25519Keyring::Alice]);
    let grandpa_just8 = GrandpaJustification::from_commit(&client, round, commit).unwrap();

    client
        .finalize_block(block8.hash(), Some((ID, grandpa_just8.encode().clone())))
        .unwrap();

    // Authority set change at block 8, so the justification stored there will be used in the
    // FinalityProof for block 6
    let mut authority_set_changes = AuthoritySetChanges::empty();
    authority_set_changes.append(0, 5);
    authority_set_changes.append(1, 8);

    let proof_of_6: FinalityProof =
        prove_finality(&*backend, authority_set_changes.clone(), 6, true)
            .unwrap()
            .unwrap();

    assert_eq!(
        proof_of_6,
        FinalityProof {
            block: block8.hash(),
            justification: grandpa_just8.encode(),
            unknown_headers: vec![block7.header().clone(), block8.header().clone()],
        },
    );

    let proof_of_6_without_unknown: FinalityProof =
        prove_finality(&*backend, authority_set_changes.clone(), 6, false)
            .unwrap()
            .unwrap();

    assert_eq!(
        proof_of_6_without_unknown,
        FinalityProof {
            block: block8.hash(),
            justification: grandpa_just8.encode(),
            unknown_headers: vec![],
        },
    );
}

#[test]
fn finality_proof_in_last_set_fails_without_latest() {
    let (_, backend, _) = test_blockchain(8, &[4, 5, 8]);

    // No recent authority set change, so we are in the latest set, and we will try to pickup
    // the best stored justification, for which there is none in this case.
    let mut authority_set_changes = AuthoritySetChanges::empty();
    authority_set_changes.append(0, 5);

    assert!(matches!(
        prove_finality(&*backend, authority_set_changes, 6, true),
        Ok(None)
    ));
}

#[test]
fn finality_proof_in_last_set_using_latest_justification_works() {
    let (client, backend, blocks) = test_blockchain(8, &[4, 5, 8]);
    let block7 = &blocks[6];
    let block8 = &blocks[7];

    let round = 8;
    let commit = create_commit(block8.clone(), round, 1, &[Ed25519Keyring::Alice]);
    let grandpa_just8 = GrandpaJustification::from_commit(&client, round, commit).unwrap();
    store_best_justification(&client, &grandpa_just8);

    // No recent authority set change, so we are in the latest set, and will pickup the best
    // stored justification
    let mut authority_set_changes = AuthoritySetChanges::empty();
    authority_set_changes.append(0, 5);

    let proof_of_6: FinalityProof = prove_finality(&*backend, authority_set_changes, 6, true)
        .unwrap()
        .unwrap();

    assert_eq!(
        proof_of_6,
        FinalityProof {
            block: block8.hash(),
            justification: grandpa_just8.encode(),
            unknown_headers: vec![block7.header().clone(), block8.header().clone()],
        }
    );
}

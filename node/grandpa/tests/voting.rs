use super::*;
use sc_block_builder::BlockBuilderBuilder;
use sp_consensus::BlockOrigin;
use sp_runtime::traits::Header as _;

use substrate_test_runtime_client::{
    runtime::{Block, Header},
    Backend, Client, ClientBlockImportExt, DefaultTestClientBuilderExt, TestClientBuilder,
    TestClientBuilderExt,
};

/// A mock voting rule that subtracts a static number of block from the `current_target`.
#[derive(Clone)]
struct Subtract(u64);
impl VotingRule<Block, Client<Backend>> for Subtract {
    fn restrict_vote(
        &self,
        backend: Arc<Client<Backend>>,
        _base: &Header,
        _best_target: &Header,
        current_target: &Header,
    ) -> VotingRuleResult<Block> {
        let target_number = current_target.number() - self.0;
        let res = backend
            .hash(target_number)
            .unwrap()
            .map(|target_hash| (target_hash, target_number));

        Box::pin(std::future::ready(res))
    }
}

#[test]
fn multiple_voting_rules_cannot_restrict_past_base() {
    // setup an aggregate voting rule composed of two voting rules
    // where each subtracts 50 blocks from the current target
    let rule = VotingRulesBuilder::new()
        .add(Subtract(50))
        .add(Subtract(50))
        .build();

    let client = Arc::new(TestClientBuilder::new().build());
    let mut hashes = Vec::with_capacity(200);

    for _ in 0..200 {
        let block = BlockBuilderBuilder::new(&*client)
            .on_parent_block(client.chain_info().best_hash)
            .with_parent_block_number(client.chain_info().best_number)
            .build()
            .unwrap()
            .build()
            .unwrap()
            .block;
        hashes.push(block.hash());

        futures::executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();
    }

    let genesis = client.header(client.info().genesis_hash).unwrap().unwrap();

    let best = client.header(client.info().best_hash).unwrap().unwrap();

    let (_, number) =
        futures::executor::block_on(rule.restrict_vote(client.clone(), &genesis, &best, &best))
            .unwrap();

    // we apply both rules which should subtract 100 blocks from best block (#200)
    // which means that we should be voting for block #100
    assert_eq!(number, 100);

    let block110 = client.header(hashes[109]).unwrap().unwrap();

    let (_, number) =
        futures::executor::block_on(rule.restrict_vote(client.clone(), &block110, &best, &best))
            .unwrap();

    // base block is #110 while best block is #200, applying both rules would make
    // would make the target block (#100) be lower than the base block, therefore
    // only one of the rules is applied.
    assert_eq!(number, 150);
}

#[test]
fn before_best_by_has_cutoff_at_base() {
    let rule = BeforeBestBlockBy(2);

    let client = Arc::new(TestClientBuilder::new().build());

    let n = 5;
    let mut hashes = Vec::with_capacity(n);
    for _ in 0..n {
        let block = BlockBuilderBuilder::new(&*client)
            .on_parent_block(client.chain_info().best_hash)
            .with_parent_block_number(client.chain_info().best_number)
            .build()
            .unwrap()
            .build()
            .unwrap()
            .block;
        hashes.push(block.hash());

        futures::executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();
    }

    let best = client.header(client.info().best_hash).unwrap().unwrap();
    let best_number = *best.number();

    for i in 0..n {
        let base = client.header(hashes[i]).unwrap().unwrap();
        let (_, number) =
            futures::executor::block_on(rule.restrict_vote(client.clone(), &base, &best, &best))
                .unwrap();

        let expected = std::cmp::max(best_number - 2, *base.number());
        assert_eq!(
            number, expected,
            "best = {}, lag = 2, base = {}",
            best_number, i
        );
    }
}

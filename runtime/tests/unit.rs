//! 总装配常量、标签与块类型的单元约束。
use super::*;

#[test]
fn time_and_currency_constants_are_consistent() {
    use frame_support::traits::Get;

    assert_eq!(YUAN, 100 * FEN);
    assert_eq!(UNIT, YUAN);
    assert_eq!(primitives::pow_const::POW_TARGET_BLOCK_TIME_MS, 360_000);
    let minimum_period: u64 = <Runtime as pallet_timestamp::Config>::MinimumPeriod::get();
    assert_eq!(minimum_period, 1);
}

#[test]
fn governance_module_tags_are_globally_unique() {
    use std::collections::HashSet;
    let tags: [(&str, &[u8]); 9] = [
        ("grandpakey", grandpakey::MODULE_TAG),
        ("destroy", destroy::MODULE_TAG),
        ("resolutionissue", resolutionissue::MODULE_TAG),
        ("upgrade", upgrade::MODULE_TAG),
        ("publicentity", publicentity::MODULE_TAG),
        ("privateentity", privateentity::MODULE_TAG),
        ("personaladmin", personaladmin::MODULE_TAG),
        ("multisig", multisig::MODULE_TAG),
        ("legislation", legislation::MODULE_TAG),
    ];
    let unique: HashSet<&[u8]> = tags.iter().map(|(_, t)| *t).collect();
    assert_eq!(
        unique.len(),
        tags.len(),
        "MODULE_TAG must be globally unique across governance pallets; got: {:?}",
        tags,
    );
}

#[test]
fn runtime_version_and_block_types_are_sane() {
    assert_eq!(VERSION.spec_name.as_ref(), "citizenchain");
    assert_eq!(VERSION.impl_name.as_ref(), "citizenchain");
    assert_eq!(VERSION.authoring_version, 0);
    assert_eq!(VERSION.spec_version, 0);
    assert_eq!(VERSION.impl_version, 0);
    assert_eq!(VERSION.transaction_version, 0);
    assert_eq!(VERSION.system_version, 0);

    let _opaque_block_id: opaque::BlockId = generic::BlockId::Number(0);
    let _runtime_block_id: BlockId = generic::BlockId::Number(0);
}

#[test]
fn pow_digest_author_finds_pow_engine_author() {
    // pre_digest 现在存储 sr25519 公钥，PowDigestAuthor 解码后派生 AccountId。
    let public = sp_core::sr25519::Public::from_raw([21u8; 32]);
    let expected_account: AccountId = sp_runtime::MultiSigner::from(public).into_account();
    let encoded = public.encode();
    let digests: Vec<(sp_runtime::ConsensusEngineId, &[u8])> = vec![
        (*b"TEST", b"ignored".as_ref()),
        (sp_consensus_pow::POW_ENGINE_ID, encoded.as_slice()),
    ];
    let found = PowDigestAuthor::find_author(digests);
    assert_eq!(found, Some(expected_account));
}

#[test]
fn maintenance_allocations_leave_space_for_other_hooks() {
    use frame_support::traits::Get;
    let max = <Runtime as frame_system::Config>::BlockWeights::get().max_block;
    let vote = <Runtime as votebase::Config>::MaxExecutionWeightPerBlock::get();
    let legal = <Runtime as legislation::Config>::ActivationWeightBudget::get();
    let identity = <Runtime as identity::Config>::MaxPopulationMaintenanceWeightPerBlock::get();
    let square =
        frame_support::weights::Weight::from_parts(max.ref_time() / 32, max.proof_size() / 32);
    assert!(!vote
        .saturating_add(legal)
        .saturating_add(identity)
        .saturating_add(square)
        .any_gt(max));
}

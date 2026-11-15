//! 运行时交易与阶段切换的专属测试，入口和夹具均归本模块。
#![allow(clippy::expect_used, clippy::unwrap_used)]

use frame_support::traits::Currency;
use runtime::{
    AccountId, Address, Balances, CheckNonStakeSender, Executive, GenesisPallet, Mldsa,
    Runtime, RuntimeCall, RuntimeGenesisConfig, SignedPayload, System, TxExtension,
    UncheckedExtrinsic,
};
use sp_core::{sr25519, Pair};
use sp_runtime::{generic, BuildStorage};
use codec::Encode;
use sp_runtime::{
    generic::Preamble,
    traits::{ExtensionVariant, PipelineAtVers, TransactionExtension},
    transaction_validity::TransactionSource,
};

// 保留原交易测试的初始化条件，夹具不再依赖外部测试模块。
fn new_test_ext() -> sp_io::TestExternalities {
    let storage = RuntimeGenesisConfig::default()
        .build_storage()
        .expect("运行时交易测试的创世存储构造失败");
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
        pallet_timestamp::Pallet::<Runtime>::set_timestamp(1_782_950_400_000);
        identity::PopulationReadyDate::<Runtime>::put(20260702);
    });
    ext
}

fn extra(nonce: u32) -> TxExtension {
    (
        frame_system::AuthorizeCall::new(),
        frame_system::CheckNonZeroSender::new(),
        CheckNonStakeSender,
        frame_system::CheckSpecVersion::new(),
        frame_system::CheckTxVersion::new(),
        frame_system::CheckGenesis::new(),
        frame_system::CheckEra::from(generic::Era::Immortal),
        frame_system::CheckNonce::from(nonce),
        frame_system::CheckWeight::new(),
        pallet_transaction_payment::ChargeTransactionPayment::from(0),
        frame_metadata_hash_extension::CheckMetadataHash::new(false),
        (Default::default(), frame_system::WeightReclaim::new()),
    )
}

fn pq_transaction(child: &[u8; 32], nonce: u32, first: bool, corrupt: bool) -> UncheckedExtrinsic {
    let public = mldsa::public_key(child);
    let account = mldsa::account_id(&public);
    let call = RuntimeCall::OnchainTransaction(onchain::pallet::Call::transfer_with_remark {
        beneficiary_account_id: AccountId::new([0x69; 32]),
        amount: 1,
        remark: Vec::new().try_into().unwrap(),
    });
    let ext = extra(nonce);
    let implicit = ext.implicit().unwrap();
    let review = (call.clone(), ext.clone(), implicit).encode();
    let mut implication = vec![mldsa::EXTENSION_VERSION];
    implication.extend_from_slice(&review);
    let mut signature =
        mldsa::sign(child, &mldsa::transaction_message(&account, &implication)).unwrap();
    if corrupt {
        signature.0[10] ^= 1;
    }
    let proof = if first {
        mldsa::Proof::First {
            account,
            public,
            signature,
        }
    } else {
        mldsa::Proof::Stored { account, signature }
    };
    generic::UncheckedExtrinsic::from_parts(
        call,
        Preamble::General(ExtensionVariant::Other(PipelineAtVers::<1, _>::new((
            mldsa::Authenticate::<Runtime>::from(proof),
            ext,
        )))),
    )
    .into()
}

#[test]
fn pq_first_use_is_read_only_in_pool_persists_once_and_survives_phase_switch() {
    new_test_ext().execute_with(|| {
        let seed = [0x45; 32];
        let public = mldsa::public_key(&seed);
        let account = AccountId::new(mldsa::account_id(&public));
        let _ = Balances::make_free_balance_be(&account, 100_000);
        let _ = Balances::make_free_balance_be(&AccountId::new([0x69; 32]), 1000);
        let unfunded_seed = [0x47; 32];
        let unfunded = AccountId::new(mldsa::account_id(&mldsa::public_key(&unfunded_seed)));
        assert!(
            Executive::apply_extrinsic(pq_transaction(&unfunded_seed, 0, true, false)).is_err()
        );
        assert!(!mldsa::PqPublicKey::<Runtime>::contains_key(&unfunded));
        let first = pq_transaction(&seed, 0, true, false);
        assert!(Executive::validate_transaction(
            TransactionSource::External,
            first.clone(),
            System::block_hash(0)
        )
        .is_ok());
        assert!(!mldsa::PqPublicKey::<Runtime>::contains_key(&account));
        assert!(Executive::apply_extrinsic(pq_transaction(&seed, 0, true, true)).is_err());
        assert!(!mldsa::PqPublicKey::<Runtime>::contains_key(&account));
        assert_eq!(Executive::apply_extrinsic(first.clone()), Ok(Ok(())));
        assert_eq!(
            mldsa::PqPublicKey::<Runtime>::get(&account),
            Some(public.clone())
        );
        assert_eq!(System::account_nonce(&account), 1);
        assert!(Executive::apply_extrinsic(first).is_err());
        let known = pq_transaction(&seed, 1, false, false);
        assert!(
            known.encode().len() + mldsa::PUBLIC_KEY_BYTES
                == pq_transaction(&seed, 1, true, false).encode().len()
        );
        assert_eq!(Executive::apply_extrinsic(known), Ok(Ok(())));
        <mldsa::GenesisChainPhase<Runtime> as mldsa::ChainPhase>::enter_operation();
        assert_eq!(
            Executive::apply_extrinsic(pq_transaction(&seed, 2, false, false)),
            Ok(Ok(()))
        );
        let unknown_seed = [0x46; 32];
        let unknown = AccountId::new(mldsa::account_id(&mldsa::public_key(&unknown_seed)));
        let _ = Balances::make_free_balance_be(&unknown, 100_000);
        assert!(
            Executive::apply_extrinsic(pq_transaction(&unknown_seed, 0, false, false)).is_err()
        );
        assert_eq!(
            Executive::apply_extrinsic(pq_transaction(&unknown_seed, 0, true, false)),
            Ok(Ok(()))
        );
        assert_eq!(mldsa::PqPublicKey::<Runtime>::get(&account), Some(public));
    });
}

#[test]
fn sr_uses_unchanged_v4_bytes_and_is_rejected_in_operation() {
    new_test_ext().execute_with(|| {
        let pair = sr25519::Pair::from_seed(&[0x51; 32]);
        let account = AccountId::new(pair.public().0);
        let _ = Balances::make_free_balance_be(&account, 100_000);
        let call = RuntimeCall::System(frame_system::Call::remark { remark: vec![1] });
        let ext = extra(0);
        let payload = SignedPayload::new(call.clone(), ext.clone()).unwrap();
        let signature = payload.using_encoded(|bytes| pair.sign(bytes));
        let transaction: UncheckedExtrinsic = generic::UncheckedExtrinsic::new_signed(
            call,
            Address::Id(account.clone()),
            signature.into(),
            ext,
        )
        .into();
        assert!(Executive::validate_transaction(
            TransactionSource::External,
            transaction.clone(),
            System::block_hash(0)
        )
        .is_ok());
        <mldsa::GenesisChainPhase<Runtime> as mldsa::ChainPhase>::enter_operation();
        assert!(Executive::apply_extrinsic(transaction).is_err());
        assert!(!mldsa::PqPublicKey::<Runtime>::contains_key(account));
    });
}

#[cfg(feature = "operation")]
#[test]
fn operation_upgrade_switches_once_without_rewriting_accounts() {
    use frame_support::traits::OnRuntimeUpgrade;
    new_test_ext().execute_with(|| {
        assert_eq!(GenesisPallet::phase(), ::genesis::ChainPhase::Genesis);
        let account = AccountId::new([0x61; 32]);
        let _ = Balances::make_free_balance_be(&account, 100_000);
        Mldsa::on_runtime_upgrade();
        assert_eq!(GenesisPallet::phase(), ::genesis::ChainPhase::Operation);
        assert!(!GenesisPallet::developer_upgrade_enabled());
        assert_eq!(Balances::free_balance(&account), 100_000);
        let before = sp_io::storage::root(sp_runtime::StateVersion::V1);
        Mldsa::on_runtime_upgrade();
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), before);
    });
}

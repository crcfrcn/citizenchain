//! 主Runtime实际类型装配下的独立测试域。

use super::*;
use codec::Encode;
use frame_support::traits::fungible::Mutate;
use pallet_revive::{evm::fees::NativeFee, AddressMapper, H160, U256};
use sp_runtime::{
    traits::{Checkable, Dispatchable, Header as _},
    transaction_validity::InvalidTransaction,
};

const GAS: u64 = 100_000_000;
// 测试预算同时覆盖最低资源额度与金额对应费用，价格仍由真实 Runtime 报价。
fn fee_gas(amount: Balance) -> u64 {
    pallet_revive::evm::fees::native_fee_to_gas::<Runtime>(
        primitives::fee_policy::calculate_onchain_fee(amount),
    )
    .expect("测试金额的费用 gas 必须可准确表达")
}
fn signer() -> AccountId {
    pallet_revive::evm::Account::default().substrate_account()
}
fn fee_accounts() -> [AccountId; 3] {
    [
        AccountId::new([0x73; 32]),
        AccountId::new(primitives::cid::china::china_cb::CHINA_CB[0].fee_account),
        AccountId::new(primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT),
    ]
}
fn ext() -> sp_io::TestExternalities {
    let mut ext = new_test_ext();
    ext.execute_with(|| {
        let miner = sr25519::Pair::from_string("//Alice//pow", None).unwrap();
        let miner_account = MultiSigner::from(miner.public()).into_account();
        let mut digest = sp_runtime::Digest::default();
        digest.push(sp_runtime::DigestItem::PreRuntime(
            sp_consensus_pow::POW_ENGINE_ID,
            miner.public().encode(),
        ));
        // 共用夹具已处于第 1 块，Executive 按真实规则初始化下一块。
        let header = Header::new(
            2,
            Default::default(),
            Default::default(),
            Default::default(),
            digest,
        );
        Executive::initialize_block(&header);
        Timestamp::set_timestamp(1_782_950_400_000);
        Balances::set_balance(&signer(), 10_000_000);
        for account in fee_accounts() {
            Balances::set_balance(&account, 1_000);
        }
        fullnodeissue::RewardAccountIdByMiner::<Runtime>::insert(
            miner_account,
            fee_accounts()[0].clone(),
        );
    });
    ext
}
fn tx(to: Option<H160>, input: Vec<u8>, amount: Balance) -> pallet_revive::evm::GenericTransaction {
    pallet_revive::evm::GenericTransaction {
        from: Some(pallet_revive::evm::Account::default().address()),
        to,
        input: pallet_revive::evm::Bytes(input).into(),
        chain_id: Some(primitives::core_const::ETHEREUM_CHAIN_ID.into()),
        nonce: Some(System::account_nonce(signer()).into()),
        gas: Some(GAS.max(fee_gas(amount)).into()),
        gas_price: Some(Revive::evm_base_fee()),
        value: Some(U256::from(amount) * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO)),
        r#type: Some(pallet_revive::evm::TYPE_LEGACY.into()),
        ..Default::default()
    }
}
fn extrinsic(tx: pallet_revive::evm::GenericTransaction) -> UncheckedExtrinsic {
    let signed =
        pallet_revive::evm::Account::default().sign_transaction(tx.try_into_unsigned().unwrap());
    generic::UncheckedExtrinsic::new_bare(RuntimeCall::Revive(pallet_revive::Call::eth_transact {
        payload: signed.signed_payload(),
    }))
    .into()
}
/// 与BlockBuilder一致：无效交易丢弃执行覆盖层；合约REVERT/OOG返回外层Ok，保留原费用。
fn apply_checked(xt: UncheckedExtrinsic) -> sp_runtime::ApplyExtrinsicResult {
    frame_support::storage::with_transaction_unchecked(|| {
        let result = Executive::apply_extrinsic(xt);
        if result.is_ok() {
            sp_runtime::TransactionOutcome::Commit(result)
        } else {
            sp_runtime::TransactionOutcome::Rollback(result)
        }
    })
}
fn apply(tx: pallet_revive::evm::GenericTransaction) {
    let result = apply_checked(extrinsic(tx));
    assert!(matches!(result, Ok(Ok(_))), "{result:?}");
}
fn initcode(runtime: &[u8]) -> Vec<u8> {
    assert!(runtime.len() < 256);
    let n = runtime.len() as u8;
    let mut code = vec![0x60, n, 0x60, 12, 0x60, 0, 0x39, 0x60, n, 0x60, 0, 0xf3];
    code.extend_from_slice(runtime);
    code
}
fn deploy(runtime: &[u8]) -> H160 {
    let address = pallet_revive::evm::Account::default().address();
    let nonce = System::account_nonce(signer());
    // CREATE 地址来自真实已恢复签名者及共享 System nonce。
    let dest = pallet_revive::create1(&address, nonce.into());
    apply(tx(None, initcode(runtime), 200));
    assert_eq!(Revive::code(&dest), runtime);
    dest
}
fn paid_fees() -> Vec<(AccountId, u128)> {
    System::events()
        .into_iter()
        .filter_map(|event| match event.event {
            RuntimeEvent::OnchainTransaction(onchain::pallet::Event::FeePaid {
                account_id,
                fee,
            }) => Some((account_id, fee)),
            _ => None,
        })
        .collect()
}
#[test]
fn standard_precompiles_execute_with_native_fees_and_read_only_simulation() {
    ext().execute_with(|| {
            // 固定共识向量验证真实 Runtime 入口；预编译地址不另设收费或账户账本。
            let mut modexp = vec![0u8; 96];
            modexp[31] = 1; modexp[63] = 1; modexp[95] = 1;
            modexp.extend_from_slice(&[3, 2, 5]);
            let blake = hex::decode("0000000c48c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000001").unwrap();
            let p256 = hex::decode("4cee90eb86eaa050036147a12d49004b6b9c72bd725d39d4785011fe190f0b4da73bd4903f0ce3b639bbbf6e8e80d16931ff4bcf5993d58468e8fb19086e8cac36dbcd03009df8c59286b162af3bd7fcc0450c9aa81be5d10d312af6c66b1d604aebd3099c618202fcfe16ae7770b0c49ab5eadf74b754204a3bb6060e44eff37618b065f9832de4ca6ca971a7a1adc826d0f7c00181a5fb2ddf79ae00b4e10e").unwrap();
            let ecrecover = hex::decode("18c547e4f7b0f325ad1e56f57e26c745b09a3e503d86e00e5255ff7f715d3d1c000000000000000000000000000000000000000000000000000000000000001c73b1693892219d736caba55bdb67216e485557ea6b6af75f37096c9aa6a5a75feeb940b1d03b21e36b0e47e79769f095fe2ab855bd91e3a38756b7d75a9c4549").unwrap();
            let mut one = vec![0; 32]; one[31] = 1;
            let vectors = [
                (1u64, ecrecover, hex::decode("000000000000000000000000a94f5374fce5edbc8e2a8697c15331677e6ebf0b").unwrap()),
                (2, b"abc".to_vec(), sp_io::hashing::sha2_256(b"abc").to_vec()),
                (3, b"abc".to_vec(), hex::decode("0000000000000000000000008eb208f7e05d987a9b044a8e98c6b087f15a0bfc").unwrap()),
                (4, b"abc".to_vec(), b"abc".to_vec()),
                (5, modexp, vec![4]),
                (6, vec![], vec![0; 64]),
                (7, vec![], vec![0; 64]),
                (8, vec![], one.clone()),
                (9, blake, hex::decode("ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923").unwrap()),
                (0x100, p256, one),
            ];
            for (address, input, output) in vectors {
                let transaction = tx(Some(H160::from_low_u64_be(address)), input, 0);
                let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
                let result = Revive::dry_run_eth_transact(transaction.clone(), Default::default()).unwrap();
                assert_eq!(result.data, output, "预编译地址 {address:#x}");
                assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
                let before = Balances::free_balance(signer());
                System::reset_events();
                apply(transaction);
                let fee = primitives::fee_policy::calculate_onchain_fee(0);
                assert_eq!(Balances::free_balance(signer()), before - fee);
                assert_eq!(paid_fees(), vec![(signer(), fee)]);
            }
        });
}

#[test]
fn invalid_standard_precompile_input_and_unsupported_kzg_preserve_state_in_simulation() {
    ext().execute_with(|| {
        for (address, input) in [
            (6, vec![0xff; 128]),
            (8, vec![1]),
            (9, vec![1]),
            (0x0a, vec![0; 192]),
        ] {
            let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
            assert!(Revive::dry_run_eth_transact(
                tx(Some(H160::from_low_u64_be(address)), input, 0),
                Default::default()
            )
            .is_err());
            assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
        }
        // P256 无效签名按官方合同返回空值，不能误报验证成功。
        let result = Revive::dry_run_eth_transact(
            tx(Some(H160::from_low_u64_be(0x100)), vec![0; 160], 0),
            Default::default(),
        )
        .unwrap();
        assert!(result.data.is_empty());
    });
}

#[test]
fn signed_eoa_transfers_keep_existing_amount_fee_payer_and_distribution() {
    ext().execute_with(|| {
        let dest = H160::repeat_byte(0x49);
        let receiver = pallet_revive::AccountId32Mapper::<Runtime>::to_account_id(&dest);
        Balances::set_balance(&receiver, EXISTENTIAL_DEPOSIT);
        for kind in [
            pallet_revive::evm::TYPE_LEGACY,
            pallet_revive::evm::TYPE_EIP2930,
            pallet_revive::evm::TYPE_EIP1559,
        ] {
            System::reset_events();
            let amount = 20_000;
            let fee = primitives::fee_policy::calculate_onchain_fee(amount);
            let before = Balances::free_balance(signer());
            let receiver_before = Balances::free_balance(&receiver);
            let shares = fee_accounts().map(|account| Balances::free_balance(account));
            let mut transaction = tx(Some(dest), vec![], amount);
            transaction.r#type = Some(kind.into());
            transaction.max_fee_per_gas = Some(primitives::core_const::NATIVE_TO_ETH_RATIO.into());
            transaction.max_priority_fee_per_gas = Some(U256::zero());
            apply(transaction);
            assert_eq!(Balances::free_balance(signer()), before - amount - fee);
            assert_eq!(Balances::free_balance(&receiver), receiver_before + amount);
            assert_eq!(paid_fees(), vec![(signer(), fee)]);
            assert_eq!(
                fee_accounts().map(|account| Balances::free_balance(account)),
                [shares[0] + 16, shares[1] + 2, shares[2] + 2]
            );
        }
        assert_eq!(System::account_nonce(signer()), 3);
    });
}
#[test]
fn signed_deployment_storage_logs_and_internal_call_charge_outer_transaction_once() {
    ext().execute_with(|| {
        // SSTORE(0,42)，LOG1(topic7,data42)，CALL(calldata中的地址, value0)。
        let code = vec![
            0x60, 42, 0x60, 0, 0x55, 0x60, 42, 0x60, 0, 0x52, 0x60, 7, 0x60, 32, 0x60, 0, 0xa1,
            0x60, 0, 0x60, 0, 0x60, 0, 0x60, 0, 0x60, 0, 0x60, 0, 0x35, 0x5a, 0xf1, 0x50, 0x00,
        ];
        let child = deploy(&code);
        let parent = deploy(&code);
        System::reset_events();
        let before = Balances::free_balance(signer());
        let mut input = vec![0; 12];
        input.extend_from_slice(child.as_bytes());
        apply(tx(Some(parent), input, 0));
        let mut value = vec![0; 32];
        value[31] = 42;
        assert_eq!(
            Revive::get_storage(parent, [0; 32]).unwrap(),
            Some(value.clone())
        );
        assert_eq!(Revive::get_storage(child, [0; 32]).unwrap(), Some(value));
        assert_eq!(
            Balances::free_balance(signer()),
            before - primitives::fee_policy::calculate_onchain_fee(0)
        );
        assert_eq!(
            paid_fees(),
            vec![(signer(), primitives::fee_policy::calculate_onchain_fee(0))]
        );
        // 父、子合约分别写真实日志。
        assert_eq!(
            System::events()
                .iter()
                .filter(|event| matches!(
                    event.event,
                    RuntimeEvent::Revive(pallet_revive::Event::ContractEmitted { .. })
                ))
                .count(),
            2
        );
    });
}
#[test]
fn simulation_and_quote_are_read_only_and_ignore_resource_price_for_actual_fee() {
    ext().execute_with(|| {
        let dest = H160::repeat_byte(0x49);
        for gas in [GAS, GAS * 2] {
            let mut transaction = tx(Some(dest), vec![], 0);
            transaction.gas = Some(gas.into());
            let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
            let checked = extrinsic(transaction.clone())
                .check(&frame_system::ChainContext::<Runtime>::default())
                .unwrap();
            assert_eq!(
                RuntimeNativeFee::quote(&signer(), &checked.function),
                Ok(primitives::fee_policy::calculate_onchain_fee(0))
            );
            assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
            let result = Revive::dry_run_eth_transact(
                transaction,
                pallet_revive::DryRunConfig {
                    perform_balance_checks: Some(true),
                    ..Default::default()
                },
            );
            assert!(result.is_ok(), "{result:?}");
            assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
        }
    });
}

#[test]
fn wallet_quotes_and_signed_transfers_cover_minimum_rounding_and_percentage_fees() {
    for (amount, fee) in [(100, 10), (10_499, 10), (10_500, 11), (100_000, 100)] {
        // 四档金额覆盖 Legacy/访问列表价格缓冲及三种合法钱包优先费。
        for (kind, priority) in [
            (pallet_revive::evm::TYPE_LEGACY, None),
            (pallet_revive::evm::TYPE_EIP2930, None),
            (pallet_revive::evm::TYPE_EIP1559, Some(U256::zero())),
            (pallet_revive::evm::TYPE_EIP1559, Some(U256::one())),
            (
                pallet_revive::evm::TYPE_EIP1559,
                Some(U256::from(1_000_000u64)),
            ),
            (
                pallet_revive::evm::TYPE_EIP1559,
                Some(U256::from(2_000_000_000u64)),
            ),
        ] {
            ext().execute_with(|| {
                let price = Revive::evm_base_fee();
                assert_eq!(price, U256::from(1_000_000_000u64));
                let dest = H160::repeat_byte(0x49);
                let receiver = pallet_revive::AccountId32Mapper::<Runtime>::to_account_id(&dest);
                Balances::set_balance(&receiver, EXISTENTIAL_DEPOSIT);
                let before = Balances::free_balance(signer());
                let receiver_before = Balances::free_balance(&receiver);
                let mut transaction = tx(Some(dest), vec![], amount);
                transaction.r#type = Some(kind.into());
                if kind == pallet_revive::evm::TYPE_EIP1559 {
                    transaction.gas_price = None;
                    transaction.max_fee_per_gas = Some(price * U256::from(2));
                    transaction.max_priority_fee_per_gas = priority;
                } else {
                    transaction.gas_price = Some(price + U256::one());
                }
                // 钱包增加 gas 缓冲或费用上限不能改变业务费或产生第二次扣费。
                transaction.gas = transaction.gas.map(|gas| gas * U256::from(2));
                let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
                let quote =
                    Revive::dry_run_eth_transact(transaction.clone(), Default::default()).unwrap();
                assert!(
                    U256::from(quote.eth_gas) * price
                        >= U256::from(fee)
                            * U256::from(primitives::core_const::NATIVE_TO_ETH_RATIO)
                );
                assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
                System::reset_events();
                apply(transaction);
                assert_eq!(Balances::free_balance(signer()), before - amount - fee);
                assert_eq!(Balances::free_balance(&receiver), receiver_before + amount);
                assert_eq!(paid_fees(), vec![(signer(), fee)]);
                assert_eq!(System::account_nonce(signer()), 1);
            });
        }
    }
}

#[test]
fn wallet_price_priority_and_signed_fee_limits_reject_before_charging() {
    ext().execute_with(|| {
        let price = Revive::evm_base_fee();
        for variant in 0..4 {
            let mut transaction = tx(Some(H160::repeat_byte(0x49)), vec![], 10_500);
            match variant {
                0 => transaction.gas_price = Some(price - U256::one()),
                1 | 2 => {
                    transaction.r#type = Some(pallet_revive::evm::TYPE_EIP1559.into());
                    transaction.gas_price = None;
                    transaction.max_fee_per_gas = Some(if variant == 1 {
                        price - U256::one()
                    } else {
                        price
                    });
                    transaction.max_priority_fee_per_gas = Some(if variant == 2 {
                        price + U256::one()
                    } else {
                        U256::zero()
                    });
                }
                _ => transaction.gas = Some((fee_gas(10_500) - 1).into()),
            }
            let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
            assert!(
                apply_checked(extrinsic(transaction)).is_err(),
                "无效费用参数 {variant}"
            );
            assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
            assert_eq!(System::account_nonce(signer()), 0);
            assert!(paid_fees().is_empty());
        }
    });
}

#[test]
fn revert_and_out_of_gas_roll_back_value_storage_and_keep_one_original_fee() {
    for (code, expected) in [
        (
            vec![0x60, 42, 0x60, 0, 0x55, 0x60, 0, 0x60, 0, 0xfd],
            pallet_revive::Error::<Runtime>::ContractReverted,
        ),
        (
            vec![0x5b, 0x60, 0, 0x56],
            pallet_revive::Error::<Runtime>::OutOfGas,
        ),
    ] {
        ext().execute_with(|| {
                let dest = deploy(&code);
                let before = Balances::free_balance(signer());
                let contract_before = Balances::free_balance(pallet_revive::AccountId32Mapper::<Runtime>::to_account_id(&dest));
                let storage_before = Revive::get_storage(dest,[0;32]).unwrap();
                let shares = fee_accounts().map(|account| Balances::free_balance(account));
                System::reset_events();
                let transaction = tx(Some(dest),vec![],20_000);
                apply(transaction);
                let fee = primitives::fee_policy::calculate_onchain_fee(20_000);
                assert_eq!(Balances::free_balance(signer()),before-fee);
                assert_eq!(Balances::free_balance(pallet_revive::AccountId32Mapper::<Runtime>::to_account_id(&dest)),contract_before);
                assert_eq!(Revive::get_storage(dest,[0;32]).unwrap(),storage_before);
                assert_eq!(paid_fees(),vec![(signer(),fee)]);
                assert_eq!(fee_accounts().map(|account| Balances::free_balance(account)),[shares[0]+16,shares[1]+2,shares[2]+2]);
                let expected: sp_runtime::DispatchError = expected.into();
                assert!(System::events().iter().any(|event| matches!(
                    event.event, RuntimeEvent::Revive(pallet_revive::Event::EthExtrinsicRevert { dispatch_error }) if dispatch_error == expected
                )));
            });
    }
}
#[test]
fn wrong_chain_fractional_amount_replay_forged_origin_and_payment_failure_are_rejected() {
    ext().execute_with(|| {
        let dest = H160::repeat_byte(0x49);
        for variant in 0..4 {
            let mut transaction = tx(Some(dest), vec![], 0);
            match variant {
                0 => transaction.chain_id = Some(1.into()),
                1 => transaction.value = Some(U256::one()),
                2 => {
                    transaction.to = Some(pallet_revive::RUNTIME_PALLETS_ADDR);
                    transaction.input = pallet_revive::evm::Bytes(
                        RuntimeCall::System(frame_system::Call::remark { remark: vec![] }).encode(),
                    )
                    .into();
                }
                _ => transaction.gas = Some(U256::MAX),
            }
            let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
            assert!(apply_checked(extrinsic(transaction)).is_err());
            assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
        }
        let transaction = tx(Some(dest), vec![], 0);
        let replay = extrinsic(transaction.clone());
        apply(transaction);
        let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
        assert_eq!(apply_checked(replay), Err(InvalidTransaction::Stale.into()));
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
        let checked = extrinsic(tx(Some(dest), vec![], 0))
            .check(&frame_system::ChainContext::<Runtime>::default())
            .unwrap();
        assert!(checked
            .function
            .clone()
            .dispatch(RuntimeOrigin::signed(signer()))
            .is_err());
        Balances::set_balance(&signer(), EXISTENTIAL_DEPOSIT);
        let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
        assert!(apply_checked(extrinsic(tx(Some(dest), vec![], 0))).is_err());
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
    });
}
#[test]
fn runtime_integrity_and_actual_space_meter_enforce_finite_limits() {
    use frame_support::traits::Hooks;
    ext().execute_with(|| {
        <Revive as Hooks<BlockNumber>>::integrity_test();
        // 循环发空日志：计算预算充足，真实空间预算仍必须终止执行并回滚日志。
        let dest = deploy(&[0x5b, 0x60, 0, 0x60, 0, 0xa0, 0x60, 0, 0x56]);
        let root = sp_io::storage::root(sp_runtime::StateVersion::V1);
        let result = Revive::bare_call(
            RuntimeOrigin::signed(signer()),
            dest,
            U256::zero(),
            pallet_revive::TransactionLimits::WeightAndDeposit {
                weight_limit: Weight::from_parts(
                    RuntimeBlockWeights::get().max_block.ref_time() / 2,
                    100_000,
                ),
                deposit_limit: 0,
            },
            vec![],
            &pallet_revive::ExecConfig::new_substrate_tx().with_dry_run(Default::default()),
        );
        assert_eq!(
            result.result,
            Err(pallet_revive::Error::<Runtime>::OutOfGas.into())
        );
        assert!(result.weight_consumed.proof_size() > 50_000);
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), root);
    });
}

//! 未开放资产业务必须失败且保持完整Runtime状态不变，防止回调产生假成功。
use super::*;
use onchainissue::{execution, monitor, proposal::*, types::AssetClass};

#[test]
fn every_unimplemented_asset_execution_and_callback_fails_without_side_effects() {
    new_test_ext().execute_with(|| {
        let account = AccountId::new([42; 32]);
        let cid = b"fixture-asset-owner".to_vec();
        let denied = onchainissue::Error::<Runtime>::NotImplemented;
        assert_noop!(
            execution::execute_issue::<Runtime>(IssueProposal {
                actor_cid_number: cid.clone(),
                execution_account_id: account.clone(),
                class: AssetClass::Plain,
                name: b"Example".to_vec(),
                symbol: b"EX".to_vec(),
                description: Vec::new(),
                decimals: 8,
                initial_supply: 100u128,
            }),
            denied
        );
        assert_noop!(
            execution::execute_mint::<Runtime>(MintProposal {
                actor_cid_number: cid.clone(),
                asset_id: 7,
                to_account_id: account.clone(),
                amount: 1u128,
            }),
            onchainissue::Error::<Runtime>::NotImplemented
        );
        assert_noop!(
            execution::execute_burn::<Runtime>(BurnProposal {
                actor_cid_number: cid.clone(),
                asset_id: 7,
                from_account_id: account.clone(),
                amount: 1u128,
            }),
            onchainissue::Error::<Runtime>::NotImplemented
        );
        assert_noop!(
            execution::execute_transfer::<Runtime>(TransferProposal {
                actor_cid_number: cid.clone(),
                asset_id: 7,
                from_account_id: account.clone(),
                to_account_id: AccountId::new([43; 32]),
                amount: 1u128,
            }),
            onchainissue::Error::<Runtime>::NotImplemented
        );
        assert_noop!(
            execution::execute_close::<Runtime>(CloseProposal {
                actor_cid_number: cid.clone(),
                asset_id: 7,
            }),
            onchainissue::Error::<Runtime>::NotImplemented
        );
        let freeze = MonitorFreezeProposal {
            actor_cid_number: cid.clone(),
            asset_id: 7,
            account_id: account.clone(),
            reason_hash: [5; 32],
        };
        assert_noop!(
            monitor::execute_monitor_freeze::<Runtime>(freeze.clone()),
            onchainissue::Error::<Runtime>::NotImplemented
        );
        assert_noop!(
            monitor::execute_monitor_unfreeze::<Runtime>(freeze),
            onchainissue::Error::<Runtime>::NotImplemented
        );
        assert_noop!(
            monitor::execute_monitor_confiscate::<Runtime>(MonitorConfiscateProposal {
                actor_cid_number: cid.clone(),
                asset_id: 7,
                account_id: account.clone(),
                amount: 1u128,
                reason_hash: [5; 32],
            }),
            onchainissue::Error::<Runtime>::NotImplemented
        );
        assert_noop!(
            monitor::execute_monitor_force_transfer::<Runtime>(MonitorForceTransferProposal {
                actor_cid_number: cid.clone(),
                asset_id: 7,
                from_account_id: account,
                to_account_id: AccountId::new([43; 32]),
                amount: 1u128,
                reason_hash: [5; 32],
            }),
            onchainissue::Error::<Runtime>::NotImplemented
        );
        assert_noop!(
            monitor::execute_monitor_force_close::<Runtime>(MonitorForceCloseProposal {
                actor_cid_number: cid,
                asset_id: 7,
                reason_hash: [5; 32],
            }),
            onchainissue::Error::<Runtime>::NotImplemented
        );
        for action in [
            ACTION_ONCHAIN_ASSET_ISSUE,
            ACTION_ONCHAIN_ASSET_MINT,
            ACTION_ONCHAIN_ASSET_BURN,
            ACTION_ONCHAIN_ASSET_CLOSE,
            ACTION_ONCHAIN_ASSET_TRANSFER,
            *b"BAD!",
        ] {
            assert_noop!(
                execution::dispatch_internal_callback::<Runtime>(action, &[]),
                onchainissue::Error::<Runtime>::NotImplemented
            );
        }
        for action in [
            ACTION_ONCHAIN_ASSET_MONITOR_FREEZE,
            ACTION_ONCHAIN_ASSET_MONITOR_UNFREEZE,
            ACTION_ONCHAIN_ASSET_MONITOR_CONFISCATE,
            ACTION_ONCHAIN_ASSET_MONITOR_FORCE_TRANSFER,
            ACTION_ONCHAIN_ASSET_MONITOR_FORCE_CLOSE,
            *b"BAD!",
        ] {
            assert_noop!(
                monitor::dispatch_joint_callback::<Runtime>(action, &[255]),
                onchainissue::Error::<Runtime>::NotImplemented
            );
        }
        let before = sp_io::storage::root(sp_runtime::StateVersion::V1);
        monitor::process_force_close_schedule_on_finalize::<Runtime>(System::block_number());
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), before);
    });
}

#[test]
fn asset_identifier_exhaustion_is_atomic_and_successful_allocations_are_monotonic() {
    new_test_ext().execute_with(|| {
        onchainissue::NextAssetId::<Runtime>::put(u32::MAX);
        let before = sp_io::storage::root(sp_runtime::StateVersion::V1);
        assert!(matches!(
            execution::allocate_asset_id::<Runtime>(),
            Err(onchainissue::Error::<Runtime>::AssetIdOverflow)
        ));
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), before);
        onchainissue::NextAssetId::<Runtime>::put(41);
        assert!(matches!(execution::allocate_asset_id::<Runtime>(), Ok(41)));
        assert!(matches!(execution::allocate_asset_id::<Runtime>(), Ok(42)));
        assert_eq!(onchainissue::NextAssetId::<Runtime>::get(), 43);
    });
}

//! Setup code for [`super::command`] which would otherwise bloat that module.
//!
//! Should only be used for benchmarking as it may break in other contexts.

// Substrate benchmark CLI 固定返回 sc_cli::Error；此处不能改变上游命令接口的错误类型。
#![allow(clippy::result_large_err)]

use crate::core::service::FullClient;

use citizenchain as runtime;
use runtime::{AccountId, Balance, SystemCall};
use sc_cli::Result;
use sc_client_api::BlockBackend;
use sp_inherents::{InherentData, InherentDataProvider};
use sp_keyring::Sr25519Keyring;
use sp_runtime::OpaqueExtrinsic;

use std::{sync::Arc, time::Duration};

/// Generates extrinsics for the `benchmark overhead` command.
///
/// Note: Should only be used for benchmarking.
pub struct RemarkBuilder {
    client: Arc<FullClient>,
}

impl RemarkBuilder {
    /// Creates a new [`Self`] from the given client.
    pub fn new(client: Arc<FullClient>) -> Self {
        Self { client }
    }
}

impl frame_benchmarking_cli::ExtrinsicBuilder for RemarkBuilder {
    fn pallet(&self) -> &str {
        "system"
    }

    fn extrinsic(&self) -> &str {
        "remark"
    }

    fn build(&self, nonce: u32) -> std::result::Result<OpaqueExtrinsic, &'static str> {
        let acc = Sr25519Keyring::Bob.pair();
        let extrinsic: OpaqueExtrinsic = create_benchmark_extrinsic(
            self.client.as_ref(),
            acc,
            SystemCall::remark { remark: vec![] }.into(),
            nonce,
        )
        .into();

        Ok(extrinsic)
    }
}

/// Generates `OnchainTransaction::transfer_with_remark` extrinsics for the benchmarks.
///
/// Note: Should only be used for benchmarking.
pub struct TransferWithRemarkBuilder {
    client: Arc<FullClient>,
    dest: AccountId,
    value: Balance,
}

impl TransferWithRemarkBuilder {
    /// Creates a new [`Self`] from the given client.
    pub fn new(client: Arc<FullClient>, dest: AccountId, value: Balance) -> Self {
        Self {
            client,
            dest,
            value,
        }
    }
}

impl frame_benchmarking_cli::ExtrinsicBuilder for TransferWithRemarkBuilder {
    fn pallet(&self) -> &str {
        "onchain_transaction"
    }

    fn extrinsic(&self) -> &str {
        "transfer_with_remark"
    }

    fn build(&self, nonce: u32) -> std::result::Result<OpaqueExtrinsic, &'static str> {
        let acc = Sr25519Keyring::Bob.pair();
        let extrinsic: OpaqueExtrinsic = create_benchmark_extrinsic(
            self.client.as_ref(),
            acc,
            runtime::RuntimeCall::OnchainTransaction(onchain::pallet::Call::transfer_with_remark {
                beneficiary_account_id: self.dest.clone(),
                amount: self.value,
                remark: Default::default(),
            }),
            nonce,
        )
        .into();

        Ok(extrinsic)
    }
}

/// Create a transaction using the given `call`.
///
/// Note: Should only be used for benchmarking.
pub fn create_benchmark_extrinsic(
    client: &FullClient,
    sender: sp_core::sr25519::Pair,
    call: runtime::RuntimeCall,
    nonce: u32,
) -> runtime::UncheckedExtrinsic {
    let genesis_hash = client.block_hash(0).ok().flatten().unwrap_or_default();
    chain_signing::build_signed_extrinsic_local(call, genesis_hash, nonce, &sender)
}

/// Generates inherent data for the `benchmark overhead` command.
///
/// Note: Should only be used for benchmarking.
pub fn inherent_benchmark_data() -> Result<InherentData> {
    let mut inherent_data = InherentData::new();
    let d = Duration::from_millis(0);
    let timestamp = sp_timestamp::InherentDataProvider::new(d.into());

    futures::executor::block_on(timestamp.provide_inherent_data(&mut inherent_data))
        .map_err(|e| format!("creating inherent data: {e:?}"))?;
    Ok(inherent_data)
}

//! 定义节点全客户端、后端及服务共享Rust类型。

#![allow(clippy::result_large_err)]
use runtime;

use runtime::apis::RuntimeApi;
use runtime::opaque::Block;

use sc_telemetry::Telemetry;

pub type FullClient = sc_service::TFullClient<
    Block,
    RuntimeApi,
    sc_executor::WasmExecutor<sp_io::SubstrateHostFunctions>,
>;

pub type FullBackend = sc_service::TFullBackend<Block>;

pub type FullSelectChain = sc_consensus::LongestChain<FullBackend, Block>;

pub type Service = sc_service::PartialComponents<
    FullClient,
    FullBackend,
    FullSelectChain,
    sc_consensus::DefaultImportQueue<Block>,
    sc_transaction_pool::TransactionPoolHandle<Block, FullClient>,
    (
        sc_consensus_grandpa::GrandpaBlockImport<FullBackend, Block, FullClient, FullSelectChain>,
        sc_consensus_grandpa::LinkHalf<Block, FullClient, FullSelectChain>,
        Option<Telemetry>,
    ),
>;

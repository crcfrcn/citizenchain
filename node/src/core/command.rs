// Substrate CLI runner 的闭包签名固定返回 sc_cli::Error，无法在 Node 侧装箱该错误类型。
#![allow(clippy::result_large_err)]

use super::{
    benchmarking::{inherent_benchmark_data, RemarkBuilder, TransferWithRemarkBuilder},
    chain_spec,
    cli::{Cli, Subcommand},
    service,
};
use citizenchain::{Block, EXISTENTIAL_DEPOSIT};
use frame_benchmarking_cli::{BenchmarkCmd, ExtrinsicFactory, SUBSTRATE_REFERENCE_HARDWARE};
use primitives::core_const::{SS58_FORMAT, SUPPORT_URL};
use sc_cli::SubstrateCli;
use sc_service::PartialComponents;
use sp_core::crypto::{set_default_ss58_version, Ss58AddressFormat};
use sp_keyring::Sr25519Keyring;

impl SubstrateCli for Cli {
    fn impl_name() -> String {
        "Substrate Node".into()
    }

    fn impl_version() -> String {
        env!("SUBSTRATE_CLI_IMPL_VERSION").into()
    }

    fn description() -> String {
        env!("CARGO_PKG_DESCRIPTION").into()
    }

    fn author() -> String {
        env!("CARGO_PKG_AUTHORS").into()
    }

    fn support_url() -> String {
        SUPPORT_URL.into()
    }

    fn copyright_start_year() -> i32 {
        2017
    }

    fn load_spec(&self, id: &str) -> Result<Box<dyn sc_service::ChainSpec>, String> {
        Ok(match id {
            // Substrate CLI 的 benchmark 默认传入 dev/local/staging 等内置别名。
            // CitizenChain 不维护独立临时 dev genesis,这些别名统一落到冻结 chainspec。
            "" | "citizenchain" | "dev" | "local" | "staging" => {
                Box::new(chain_spec::chain_config()?)
            }
            // 仅供本机 clean-run / bake 流程导出隔离 fresh plain chainspec 使用。
            "citizenchain-fresh" => Box::new(chain_spec::fresh_genesis_config()?),
            path => Box::new(chain_spec::ChainSpec::from_json_file(
                std::path::PathBuf::from(path),
            )?),
        })
    }
}

/// Parse and run command line arguments
pub fn run() -> sc_cli::Result<()> {
    // 统一 CLI/chain-spec 序列化时的地址显示前缀，避免默认回落到 42。
    set_default_ss58_version(Ss58AddressFormat::custom(SS58_FORMAT));

    let mut cli = Cli::from_args();
    let pool_type_explicit =
        std::env::args().any(|arg| arg == "--pool-type" || arg.starts_with("--pool-type="));
    if !pool_type_explicit {
        // 当前本链普通节点默认不需要 fork-aware 多视图交易池。
        // 上游 fork-aware 后台子任务在本链 fresh/普通启动场景会提前结束并触发
        // `txpool-background` essential task 关闭服务；默认固定为更稳定的 SingleState。
        cli.run.pool_config.pool_type = sc_cli::TransactionPoolType::SingleState;
    }

    match &cli.subcommand {
        Some(Subcommand::Key(cmd)) => cmd.run(&cli),
        Some(Subcommand::CheckBlock(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    task_manager,
                    import_queue,
                    ..
                } = service::new_partial(&config)?;
                Ok((cmd.run(client, import_queue), task_manager))
            })
        }
        Some(Subcommand::ExportChainSpec(cmd)) => {
            let chain_spec = cli.load_spec(&cmd.chain)?;
            cmd.run(chain_spec)
        }
        Some(Subcommand::ExportBlocks(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    task_manager,
                    ..
                } = service::new_partial(&config)?;
                Ok((cmd.run(client, config.database), task_manager))
            })
        }
        Some(Subcommand::ExportState(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    task_manager,
                    ..
                } = service::new_partial(&config)?;
                Ok((cmd.run(client, config.chain_spec), task_manager))
            })
        }
        Some(Subcommand::ImportBlocks(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    task_manager,
                    import_queue,
                    ..
                } = service::new_partial(&config)?;
                Ok((cmd.run(client, import_queue), task_manager))
            })
        }
        Some(Subcommand::PurgeChain(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.sync_run(|config| cmd.run(config.database))
        }
        Some(Subcommand::Revert(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    task_manager,
                    backend,
                    ..
                } = service::new_partial(&config)?;
                Ok((cmd.run(client, backend, None), task_manager))
            })
        }
        Some(Subcommand::Benchmark(cmd)) => {
            let runner = cli.create_runner(cmd)?;

            runner.sync_run(|config| {
                // This switch needs to be in the client, since the client decides
                // which sub-commands it wants to support.
                match cmd {
                    BenchmarkCmd::Pallet(cmd) => {
                        if !cfg!(feature = "runtime-benchmarks") {
                            return Err(
                                "Runtime benchmarking wasn't enabled when building the node. \
							You can enable it with `--features runtime-benchmarks`."
                                    .into(),
                            );
                        }

                        cmd.run_with_spec::<sp_runtime::traits::HashingFor<Block>, ()>(Some(
                            config.chain_spec,
                        ))
                    }
                    BenchmarkCmd::Block(cmd) => {
                        let PartialComponents { client, .. } = service::new_partial(&config)?;
                        cmd.run(client)
                    }
                    #[cfg(not(feature = "runtime-benchmarks"))]
                    BenchmarkCmd::Storage(_) => Err(
                        "Storage benchmarking can be enabled with `--features runtime-benchmarks`."
                            .into(),
                    ),
                    #[cfg(feature = "runtime-benchmarks")]
                    BenchmarkCmd::Storage(cmd) => {
                        let PartialComponents {
                            client, backend, ..
                        } = service::new_partial(&config)?;
                        let db = backend.expose_db();
                        let storage = backend.expose_storage();
                        let shared_cache = backend.expose_shared_trie_cache();

                        cmd.run(config, client, db, storage, shared_cache)
                    }
                    BenchmarkCmd::Overhead(cmd) => {
                        let PartialComponents { client, .. } = service::new_partial(&config)?;
                        let ext_builder = RemarkBuilder::new(client.clone());

                        cmd.run(
                            config.chain_spec.name().into(),
                            client,
                            inherent_benchmark_data()?,
                            Vec::new(),
                            &ext_builder,
                            false,
                        )
                    }
                    BenchmarkCmd::Extrinsic(cmd) => {
                        let PartialComponents { client, .. } = service::new_partial(&config)?;
                        // 注册 System::remark 与普通带备注转账 benchmark 构造器。
                        let ext_factory = ExtrinsicFactory(vec![
                            Box::new(RemarkBuilder::new(client.clone())),
                            Box::new(TransferWithRemarkBuilder::new(
                                client.clone(),
                                Sr25519Keyring::Alice.to_account_id(),
                                EXISTENTIAL_DEPOSIT,
                            )),
                        ]);

                        cmd.run(client, inherent_benchmark_data()?, Vec::new(), &ext_factory)
                    }
                    BenchmarkCmd::Machine(cmd) => {
                        cmd.run(&config, SUBSTRATE_REFERENCE_HARDWARE.clone())
                    }
                }
            })
        }
        Some(Subcommand::ChainInfo(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.sync_run(|config| cmd.run::<Block>(&config))
        }
        None => {
            let mining_threads = cli.mining_threads.unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1)
            });
            let gpu_device = if cli.no_gpu {
                None
            } else {
                Some(cli.gpu_device.unwrap_or(0))
            };
            let runner = cli.create_runner(&cli.run)?;
            // 固定使用 libp2p 后端承载 WSS；P2P 身份由 Noise 固定 peer ID 验证。
            // 把清算行 CLI 参数透传给 service::new_full
            let clearing_bank_cid_number = cli.clearing_bank_cid_number.clone();
            let clearing_bank_role_code = cli.clearing_bank_role_code.clone();
            let clearing_bank_password = cli.clearing_bank_password.clone();
            let clearing_reserve_monitor_interval_secs = cli.clearing_reserve_monitor_interval_secs;
            runner.run_node_until_exit(|config| async move {
                service::new_full(
                    config,
                    mining_threads,
                    gpu_device,
                    clearing_bank_cid_number,
                    clearing_bank_role_code,
                    clearing_bank_password,
                    clearing_reserve_monitor_interval_secs,
                )
                .map_err(sc_cli::Error::Service)
            })
        }
    }
}

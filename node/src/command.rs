// Substrate CLI runner 的闭包签名固定返回 sc_cli::Error，无法在 Node 侧装箱该错误类型。
#![allow(clippy::result_large_err)]
#[cfg(feature = "runtime-benchmarks")]
use crate::benchmarks::inherent_benchmark_data;
use crate::cli::Cli;
#[cfg(feature = "runtime-benchmarks")]
use frame_benchmarking_cli::BenchmarkCmd;
#[cfg(feature = "runtime-benchmarks")]
use frame_benchmarking_cli::ExtrinsicFactory;
#[cfg(feature = "runtime-benchmarks")]
use frame_benchmarking_cli::SUBSTRATE_REFERENCE_HARDWARE;
use primitives::core_const::SS58_FORMAT;
use primitives::core_const::SUPPORT_URL;
use runtime::Block;
#[cfg(feature = "runtime-benchmarks")]
use runtime::EXISTENTIAL_DEPOSIT;
use sc_cli::SubstrateCli;
use sc_service::PartialComponents;
use sp_core::crypto::set_default_ss58_version;
use sp_core::crypto::Ss58AddressFormat;
#[cfg(feature = "runtime-benchmarks")]
use sp_keyring::Sr25519Keyring;

impl SubstrateCli for Cli {
    fn impl_name() -> String {
        "Substrate Node".into()
    }

    fn impl_version() -> String {
        nodecontrol::rpc::NODE_VERSION.to_owned()
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
                Box::new(crate::chain::chain_config()?)
            }
            // 仅供本机 clean-run / bake 流程导出隔离 fresh plain chainspec 使用。
            "citizenchain-fresh" => Box::new(crate::chain::fresh_genesis_config()?),
            path => Box::new(crate::chain::ChainSpec::from_json_file(
                std::path::PathBuf::from(path),
            )?),
        })
    }
}

/// Parse and run command line arguments
pub fn run() -> sc_cli::Result<()> {
    // 统一 CLI/chain-spec 序列化时的地址显示前缀，避免默认回落到 42。
    set_default_ss58_version(Ss58AddressFormat::custom(SS58_FORMAT));

    let mut cli = crate::cli::Cli::from_args();
    let pool_type_explicit =
        std::env::args().any(|arg| arg == "--pool-type" || arg.starts_with("--pool-type="));
    if !pool_type_explicit {
        // 当前本链普通节点默认不需要 fork-aware 多视图交易池。
        // 上游 fork-aware 后台子任务在本链 fresh/普通启动场景会提前结束并触发
        // `txpool-background` essential task 关闭服务；默认固定为更稳定的 SingleState。
        cli.run.pool_config.pool_type = sc_cli::TransactionPoolType::SingleState;
    }

    match &cli.subcommand {
        Some(crate::cli::Subcommand::Key(cmd)) => cmd.run(&cli),
        Some(crate::cli::Subcommand::CheckBlock(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    task_manager,
                    import_queue,
                    ..
                } = crate::service::new_partial(&config)?;
                Ok((cmd.run(client, import_queue), task_manager))
            })
        }
        Some(crate::cli::Subcommand::ExportChainSpec(cmd)) => {
            let chain_spec = cli.load_spec(&cmd.chain)?;
            cmd.run(chain_spec)
        }
        Some(crate::cli::Subcommand::ExportBlocks(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    task_manager,
                    ..
                } = crate::service::new_partial(&config)?;
                Ok((cmd.run(client, config.database), task_manager))
            })
        }
        Some(crate::cli::Subcommand::ExportState(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    task_manager,
                    ..
                } = crate::service::new_partial(&config)?;
                Ok((cmd.run(client, config.chain_spec), task_manager))
            })
        }
        Some(crate::cli::Subcommand::ImportBlocks(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    task_manager,
                    import_queue,
                    ..
                } = crate::service::new_partial(&config)?;
                Ok((cmd.run(client, import_queue), task_manager))
            })
        }
        Some(crate::cli::Subcommand::PurgeChain(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.sync_run(|config| cmd.run(config.database))
        }
        Some(crate::cli::Subcommand::Revert(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    task_manager,
                    backend,
                    ..
                } = crate::service::new_partial(&config)?;
                Ok((cmd.run(client, backend, None), task_manager))
            })
        }
        #[cfg(feature = "runtime-benchmarks")]
        Some(crate::cli::Subcommand::Benchmark(cmd)) => {
            let runner = cli.create_runner(cmd)?;

            runner.sync_run(|config| {
                // This switch needs to be in the client, since the client decides
                // which sub-commands it wants to support.
                match cmd {
                    BenchmarkCmd::Pallet(cmd) => cmd
                        .run_with_spec::<sp_runtime::traits::HashingFor<Block>, ()>(Some(
                            config.chain_spec,
                        )),
                    BenchmarkCmd::Block(cmd) => {
                        let PartialComponents { client, .. } =
                            crate::service::new_partial(&config)?;
                        cmd.run(client)
                    }
                    BenchmarkCmd::Storage(cmd) => {
                        let PartialComponents {
                            client, backend, ..
                        } = crate::service::new_partial(&config)?;
                        let db = backend.expose_db();
                        let storage = backend.expose_storage();
                        let shared_cache = backend.expose_shared_trie_cache();

                        cmd.run(config, client, db, storage, shared_cache)
                    }
                    BenchmarkCmd::Overhead(cmd) => {
                        let PartialComponents { client, .. } =
                            crate::service::new_partial(&config)?;
                        let ext_builder = crate::benchmarks::RemarkBuilder::new(client.clone());

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
                        let PartialComponents { client, .. } =
                            crate::service::new_partial(&config)?;
                        // 注册 System::remark 与普通带备注转账 benchmark 构造器。
                        let ext_factory = ExtrinsicFactory(vec![
                            Box::new(crate::benchmarks::RemarkBuilder::new(client.clone())),
                            Box::new(crate::benchmarks::TransferWithRemarkBuilder::new(
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
        Some(crate::cli::Subcommand::ChainInfo(cmd)) => {
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
            let bank_cid = cli.bank_cid.clone();
            let bank_role = cli.bank_role.clone();
            let clearing_bank_password = cli.clearing_bank_password.clone();
            let reserve_interval_secs = cli.reserve_interval_secs;
            runner.run_node_until_exit(|config| async move {
                crate::service::new_full(
                    config,
                    mining_threads,
                    gpu_device,
                    None,
                    None,
                    bank_cid,
                    bank_role,
                    clearing_bank_password,
                    reserve_interval_secs,
                )
                .map_err(sc_cli::Error::Service)
            })
        }
    }
}

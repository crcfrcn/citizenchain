// 宿主和同机模块只通过统一公信 HTTPS 提供服务；证书任务跟随所属进程退出。
fn report_startup_error(error: &str) {
    // 标准输出是本产品公开诊断通道；只交付已去除机密的产品错误。
    println!("{}", serde_json::json!({"onchina_error": error}));
}
async fn serve_console(
    listener: std::net::TcpListener,
    app: axum::Router,
    config: axum_server::tls_rustls::RustlsConfig,
    host: bool,
    stop: tokio::sync::watch::Sender<bool>,
    mut cancel: tokio::sync::watch::Receiver<bool>,
) {
    listener
        .set_nonblocking(true)
        .unwrap_or_else(|_| panic!("HTTPS listener nonblocking configuration failed"));
    let service = app.into_make_service_with_connect_info::<SocketAddr>();
    let handle = axum_server::Handle::new();
    let shutdown = handle.clone();
    let mut renewal = tokio::spawn(crate::tls::maintain(config.clone(), host, cancel.clone()));
    let signal = tokio::spawn(async move {
        crate::tls::cancelled(&mut cancel).await;
        shutdown.graceful_shutdown(Some(std::time::Duration::from_secs(10)));
    });
    let server = axum_server::from_tcp_rustls(listener, config)
        .unwrap_or_else(|error| panic!("HTTPS listener failed: {error}"))
        .handle(handle.clone())
        .serve(service);
    tokio::pin!(server);
    let result = tokio::select! {
        result = &mut server => result.map_err(|_| "链上中国 HTTPS 服务异常退出".to_string()),
        result = &mut renewal => {
            handle.graceful_shutdown(Some(std::time::Duration::from_secs(10)));
            let _ = server.await;
            result.unwrap_or_else(|_| Err("HTTPS 证书管理任务异常退出".into()))
        }
    };
    let _ = stop.send(true);
    if !renewal.is_finished() {
        let _ = renewal.await;
    }
    signal.abort();
    if let Err(error) = result {
        report_startup_error(&error);
    }
}

/// 父管道在数据库、链读取和模块启动之前监听；独立启动不把终端EOF当退出信号。
fn parent_shutdown() -> tokio::sync::watch::Receiver<bool> {
    let (sender, receiver) = tokio::sync::watch::channel(false);
    if std::env::var_os("ONCHINA_PARENT_PIPE").is_some()
        || std::env::var_os("ONCHINA_MODULE_BIND").is_some()
    {
        std::thread::spawn(move || {
            use std::io::Read;
            let mut byte = [0u8; 1];
            loop {
                match std::io::stdin().read(&mut byte) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            let _ = sender.send(true);
        });
    }
    receiver
}

/// Ctrl-C/SIGTERM与父管道共用关闭路径；失去独立启动的发送端不等于父进程退出。
async fn shutdown_signal(mut parent: tokio::sync::watch::Receiver<bool>) {
    let parent_exit = async move {
        loop {
            if *parent.borrow() {
                return;
            }
            if parent.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .unwrap_or_else(|e| panic!("register terminate signal failed: {e}"));
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {}, _ = parent_exit => {} }
    }
    #[cfg(not(unix))]
    {
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = parent_exit => {} }
    }
}

// chain pull 端点(multisig_info / joint_vote / vote_eligibility)无 attestor 鉴权需求,
// 全局 rate limiter 防滥用,凭证签名本身就是反伪造保护。

/// 宿主管理公共资源；模块只接入已有数据库并运行自己的业务。
pub fn run_backend(local: Option<Arc<dyn modules::model::ModuleBusiness>>) {
    if std::env::args().nth(1).as_deref() == Some("--describe") {
        let value = match &local {
            Some(module) => serde_json::to_value(module.describe()),
            None => serde_json::to_value(serde_json::json!({
                "id":"platform", "interface_version":modules::model::MODULE_INTERFACE_VERSION,
                "module_directory":modules::module_directory().unwrap_or_else(|e| panic!("{e}")),
                "deployment": { "host": PLATFORM_HOST, "address": PLATFORM_ADDRESS, "url": platform_url() }
            })),
        }
        .unwrap_or_else(|e| panic!("describe backend failed: {e}"));
        println!("{value}");
        return;
    }
    disable_core_dumps();
    if local.is_none() && std::env::args().nth(1).as_deref() == Some("authorize-tls") {
        if std::env::args().len() != 2 {
            eprintln!("办证授权只接受主机终端隐藏输入，禁止通过命令参数传递");
            std::process::exit(1);
        }
        crate::tls::authorize().unwrap_or_else(|error| {
            eprintln!("{error}");
            std::process::exit(1)
        });
        return;
    }
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .compact()
        .with_writer(std::io::stderr)
        .init();
    let parent = parent_shutdown();
    let command = parse_backend_command();
    let is_host = local.is_none();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|e| panic!("build tokio runtime failed: {e}"));
    let (stop, cancel) = tokio::sync::watch::channel(false);
    let signal_sender = stop.clone();
    let signal_parent = parent.clone();
    runtime.spawn(async move {
        shutdown_signal(signal_parent).await;
        let _ = signal_sender.send(true);
    });
    // 先占用固定监听，再准备证书；地址、授权或证书失败不初始化数据库或链任务。
    let listener = if matches!(command, BackendCommand::Serve) {
        let address = if is_host {
            resolve_backend_bind_addr()
        } else {
            let address: SocketAddr = required_env("ONCHINA_MODULE_BIND")
                .parse()
                .unwrap_or_else(|_| panic!("invalid module bind"));
            assert!(
                address.ip().is_loopback(),
                "module listener must be loopback"
            );
            address
        };
        Some(
            std::net::TcpListener::bind(address).unwrap_or_else(|error| {
                let detail = match error.kind() {
                    std::io::ErrorKind::AddrInUse => format!("链上中国监听地址 {address} 已被占用"),
                    std::io::ErrorKind::AddrNotAvailable => {
                        format!("主机尚未配置链上中国固定地址 {address}")
                    }
                    _ => format!("无法监听链上中国固定地址 {address}：{}", error.kind()),
                };
                report_startup_error(&detail);
                std::process::exit(1)
            }),
        )
    } else {
        None
    };
    let tls_config = if listener.is_some() {
        match runtime.block_on(crate::tls::prepare(is_host, cancel.clone())) {
            Ok(config) => Some(config),
            Err(error) => {
                if *cancel.borrow() {
                    return;
                }
                report_startup_error(&error);
                std::process::exit(1)
            }
        }
    } else {
        None
    };

    let database_url = if is_host && crate::database_embedded::is_enabled() {
        crate::database_embedded::ensure_started()
            .unwrap_or_else(|e| panic!("embedded postgres start failed: {e}"))
    } else {
        required_env("DATABASE_URL")
    };
    if database_url
        .to_ascii_lowercase()
        .contains("sslmode=disable")
    {
        panic!("DATABASE_URL must not use sslmode=disable");
    }
    let db_is_local =
        database_url_targets_local_host_only(&database_url).unwrap_or_else(|e| panic!("{e}"));
    if !db_is_local && !env_flag_enabled("ONCHINA_ALLOW_REMOTE_DB_WITHOUT_TLS") {
        panic!("DATABASE_URL points to non-local host, but sync postgres client is running in NoTls mode; set ONCHINA_ALLOW_REMOTE_DB_WITHOUT_TLS=true only if transport is protected externally");
    }
    let db = if is_host {
        Db::from_database_url(&database_url)
    } else {
        Db::connect_existing(&database_url)
    }
    .unwrap_or_else(|e| panic!("init database failed: {e}"));
    let catalog = Arc::new(modules::ModuleCatalog::new(local.clone()));
    let state = AppState {
        db,
        rate_limiter: Arc::new(LocalRateLimiter::new()),
        modules: catalog,
    };
    if *parent.borrow() {
        if is_host {
            crate::database_embedded::stop();
        }
        return;
    }
    if is_host && matches!(command, BackendCommand::AuditChainCatalog) {
        modules::run_module_command("registry", "audit-chain-catalog", &database_url)
            .unwrap_or_else(|e| panic!("{e}"));
        return;
    }
    if run_gov_chain_projection_command(&state, command) {
        return;
    }
    init_chain_genesis_hash_blocking()
        .unwrap_or_else(|e| panic!("init chain genesis hash failed: {e}"));
    if is_host {
        let current =
            indexer::db::government::chain_projection_matches_current_head_blocking(&state.db)
                .unwrap_or_else(|e| panic!("check cid gov chain projection anchor failed: {e}"));
        if current {
            info!("cid gov chain projection is current; skip startup full sync");
        } else {
            let report = indexer::db::government::sync_gov_chain_projection_blocking(&state.db)
                .unwrap_or_else(|e| panic!("cid gov chain projection sync failed: {e}"));
            log_gov_projection_report("serve-startup", &report);
        }
        if std::env::var("ONCHINA_GOV_CHAIN_AUDIT")
            .map(|v| v != "0")
            .unwrap_or(true)
        {
            crate::audit_runtime::catalog::startup_sample_audit_blocking()
                .unwrap_or_else(|e| panic!("创世机构目录链上对账失败(fail-closed): {e}"));
        } else {
            warn!("ONCHINA_GOV_CHAIN_AUDIT=0,已跳过创世机构目录链上对账(仅限开发)");
        }
    }
    if *parent.borrow() {
        if is_host {
            crate::database_embedded::stop();
        }
        return;
    }
    if let Some(module) = &local {
        module
            .startup(&state)
            .unwrap_or_else(|e| panic!("module startup failed: {e}"));
    }
    runtime.block_on(async {
        if is_host {
            let reserved = platform_routes()
                .into_iter()
                .map(|r| r.description)
                .collect::<Vec<_>>();
            let stopped = tokio::select! {
                biased;
                _ = shutdown_signal(parent.clone()) => true,
                result = state.modules.load_installed(&database_url, &reserved) => {
                    result.unwrap_or_else(|e| panic!("module discovery failed: {e}"));
                    false
                }
            };
            if stopped {
                while let Err(error) = state.modules.stop_all() {
                    tracing::error!(%error, "modules still stopping");
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                crate::database_embedded::stop();
                return;
            }
            if crate::chain_url::chain_ws_url().is_ok() {
                tokio::spawn(indexer::indexer_worker(state.db.clone()));
                tokio::spawn(indexer::bound_institution_projection_worker(
                    state.db.clone(),
                ));
                tokio::spawn(authentication::login::revoke_stale_admin_sessions_loop(
                    state.db.clone(),
                ));
            }
        } else if let Some(module) = &local {
            module.spawn_workers(&state);
        }
        tokio::spawn(transactions::handler::recover_chain_sessions_loop(
            state.clone(),
        ));
        let app = build_routes(state.clone());
        serve_console(
            listener.expect("服务命令必须持有监听"),
            app,
            tls_config.expect("服务命令必须准备公信证书"),
            is_host,
            stop,
            cancel,
        )
        .await;
        // 没有确认子模块退出就不退出宿主；节点超时只返回失败，不能误报可替换文件。
        while let Err(error) = state.modules.stop_all() {
            tracing::error!(%error, "modules still stopping");
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        if is_host {
            crate::database_embedded::stop();
        }
    });
}

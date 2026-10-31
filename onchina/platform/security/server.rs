// 既有 HTTPS、机构证书和关闭信号处理。
/// (Card 05):内网 TLS——正式入口固定为 https://onchina.local:8964;
/// 关闭TLS必须失败；开发和正式入口都禁止明文或证书验证降级。
async fn serve_console(
    addr: SocketAddr,
    app: axum::Router,
    state: &AppState,
    parent: tokio::sync::watch::Receiver<bool>,
) {
    let service = app.into_make_service_with_connect_info::<SocketAddr>();
    // 广告 onchina.local mDNS:局域网内可经统一 HTTPS 域名访问本节点控制台(best-effort)。
    if state.modules.local.is_none() {
        discovery::advertise(addr.port());
    }
    // 中文注释：关闭 TLS 明确失败，不启动明文监听或绕过机构 CA。
    assert!(
        crate::tls::is_enabled(),
        "OnChina 必须启用 HTTPS，禁止关闭 TLS"
    );
    let config = crate::tls::load_or_generate_rustls_config()
        .await
        .unwrap_or_else(|e| panic!("onchina tls config failed: {e}"));
    info!("onchina console listening on https://{}", addr);
    let handle = axum_server::Handle::new();
    let shutdown = handle.clone();
    tokio::spawn(async move {
        shutdown_signal(parent).await;
        shutdown.graceful_shutdown(Some(std::time::Duration::from_secs(10)));
    });
    axum_server::bind_rustls(addr, config)
        .handle(handle)
        .serve(service)
        .await
        .unwrap_or_else(|err| panic!("run onchina backend https server failed: {err}"));
}

/// 下载本机构节点私有 CA 公钥证书。
///
/// 该接口必须保持未登录可访问,否则员工首次访问不可信 HTTPS 时无法先下载证书。
/// 只返回 CA 公钥证书 PEM;CA 私钥只落在服务器本地 `ONCHINA_TLS_DIR`。
async fn organization_ca_certificate() -> impl IntoResponse {
    match crate::tls::organization_ca_certificate_pem() {
        Ok(pem) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/x-x509-ca-cert"),
            );
            headers.insert(
                header::CONTENT_DISPOSITION,
                HeaderValue::from_static("attachment; filename=\"onchina-org-root-ca.crt\""),
            );
            (headers, pem).into_response()
        }
        Err(err) => {
            warn!(error = %err, "onchina CA certificate download failed");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1500,
                "onchina ca certificate unavailable",
            )
        }
    }
}

async fn organization_ca_certificate_info() -> impl IntoResponse {
    match crate::tls::organization_ca_certificate_info() {
        Ok(info) => Json(ApiResponse {
            code: 0,
            message: "ok".to_string(),
            data: OrganizationCaCertificateInfoView {
                filename: info.filename,
                sha256: info.sha256,
                subject: info.subject,
                valid_until: info.valid_until,
            },
        })
        .into_response(),
        Err(err) => {
            warn!(error = %err, "onchina CA certificate info failed");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1500,
                "onchina ca certificate unavailable",
            )
        }
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
                "module_directory":modules::module_directory().unwrap_or_else(|e| panic!("{e}"))
            })),
        }
        .unwrap_or_else(|e| panic!("describe backend failed: {e}"));
        println!("{value}");
        return;
    }
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .compact()
        .with_writer(std::io::stderr)
        .init();
    disable_core_dumps();
    let parent = parent_shutdown();
    let command = parse_backend_command();
    let is_host = local.is_none();
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
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|e| panic!("build tokio runtime failed: {e}"));
    runtime.block_on(async {
        if is_host {
            // 先准备原机构证书，所有子模块只读取同一证书目录。
            crate::tls::load_or_generate_rustls_config()
                .await
                .unwrap_or_else(|e| panic!("onchina tls config failed: {e}"));
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
        let address = if is_host {
            resolve_backend_bind_addr()
                .unwrap_or_else(|e| panic!("resolve onchina backend bind address failed: {e}"))
        } else {
            let address: SocketAddr = required_env("ONCHINA_MODULE_BIND")
                .parse()
                .unwrap_or_else(|e| panic!("invalid module bind: {e}"));
            if !address.ip().is_loopback() {
                panic!("module listener must be loopback");
            }
            address
        };
        serve_console(address, app, &state, parent.clone()).await;
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

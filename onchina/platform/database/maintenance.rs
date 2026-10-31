// 既有连接检查、命令解析与维护操作；不改变触发条件。
fn resolve_backend_bind_addr() -> SocketAddr {
    crate::PLATFORM_ADDRESS
}

fn database_url_targets_local_host_only(database_url: &str) -> Result<bool, String> {
    let config = database_url
        .parse::<postgres::Config>()
        .map_err(|e| format!("invalid DATABASE_URL: {e}"))?;
    if config.get_hosts().is_empty() {
        return Ok(true);
    }
    Ok(config.get_hosts().iter().all(|host| match host {
        Host::Tcp(name) => {
            let lowered = name.to_ascii_lowercase();
            lowered == "localhost" || lowered == "127.0.0.1" || lowered == "::1"
        }
        // postgres仅在Unix宿主提供此枚举分支；Windows保留同一TCP本机判断。
        #[cfg(unix)]
        Host::Unix(_) => true,
    }))
}

#[cfg(test)]
#[path = "../tests/database_maintenance_database_host_tests.rs"]
mod database_host_tests;

fn disable_core_dumps() {
    #[cfg(unix)]
    {
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // Best-effort hardening: avoid leaking in-memory secrets through coredumps.
        // SAFETY: `limit` 是栈上有效的 `libc::rlimit`,指针在调用期间始终有效;
        // setrlimit 只读取该结构,不保存指针,失败仅返回非 0 不产生未定义行为。
        #[allow(unsafe_code)]
        let rc = unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) };
        if rc != 0 {
            warn!(
                error = %std::io::Error::last_os_error(),
                "failed to disable core dumps"
            );
        }
    }
}

#[derive(Debug, Clone)]
enum BackendCommand {
    Serve,
    SyncGov,
    PurgeLegacyCid {
        dry_run: bool,
    },
    PurgeOrphanInstitutions {
        dry_run: bool,
        backup_path: Option<String>,
    },
    /// 创世机构目录全量链上双向比对(部署验收,ADR-031 D9)。
    AuditChainCatalog,
}

fn parse_backend_command() -> BackendCommand {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let Some(command) = args.first().map(String::as_str) else {
        return BackendCommand::Serve;
    };
    match command {
        "serve" => BackendCommand::Serve,
        "sync-gov" => BackendCommand::SyncGov,
        "audit-chain-catalog" => BackendCommand::AuditChainCatalog,
        "purge-legacy-cid" => BackendCommand::PurgeLegacyCid {
            dry_run: args.iter().any(|arg| arg == "--dry-run"),
        },
        "purge-orphan-institutions" => {
            // 默认 dry-run(只回报不删);必须显式 --apply 才落库,--apply 与
            // --dry-run 同时出现按 dry-run 处理(更安全)。--backup <path> 可覆盖默认备份文件名。
            let apply = args.iter().any(|arg| arg == "--apply");
            let dry_run = !apply || args.iter().any(|arg| arg == "--dry-run");
            BackendCommand::PurgeOrphanInstitutions {
                dry_run,
                backup_path: parse_cli_option(&args, "--backup"),
            }
        }
        other => panic!("unknown onchina command: {other}"),
    }
}

fn parse_cli_option(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].trim().to_string())
        .filter(|v| !v.is_empty())
}

fn init_chain_genesis_hash_blocking() -> Result<(), String> {
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("build chain genesis init runtime failed: {e}"))?;
    rt.block_on(crate::chain_runtime::init_genesis_hash_from_chain())
}

fn log_gov_projection_report(
    label: &'static str,
    report: &indexer::db::government::GovChainProjectionReport,
) {
    info!(
        chain_institutions = report.chain_institutions,
        chain_accounts = report.chain_accounts,
        local_institutions = report.local_institutions,
        local_accounts = report.local_accounts,
        institution_rows_changed = report.institution_rows_changed,
        account_rows_changed = report.account_rows_changed,
        obsolete_subjects_removed = report.obsolete_subjects_removed,
        obsolete_accounts_removed = report.obsolete_accounts_removed,
        chain_genesis_hash = %report.chain_genesis_hash,
        label,
        "cid gov chain projection synced"
    );
}

fn run_gov_chain_projection_command(state: &AppState, command: BackendCommand) -> bool {
    match command {
        BackendCommand::Serve => false,
        BackendCommand::AuditChainCatalog => {
            init_chain_genesis_hash_blocking()
                .unwrap_or_else(|e| panic!("init chain genesis hash failed: {e}"));
            state
                .modules
                .local
                .as_ref()
                .map(|module| module.audit_catalog())
                .unwrap_or_else(|| Err("catalog audit requires its module".into()))
                .unwrap_or_else(|e| panic!("audit-chain-catalog failed: {e}"));
            true
        }
        BackendCommand::SyncGov => {
            init_chain_genesis_hash_blocking()
                .unwrap_or_else(|e| panic!("init chain genesis hash failed: {e}"));
            let report =
                crate::indexer::db::government::sync_gov_chain_projection_blocking(&state.db)
                    .unwrap_or_else(|e| panic!("sync-gov failed: {e}"));
            log_gov_projection_report("sync-gov", &report);
            true
        }
        BackendCommand::PurgeLegacyCid { dry_run } => {
            run_purge_legacy_cid(state, dry_run);
            true
        }
        BackendCommand::PurgeOrphanInstitutions {
            dry_run,
            backup_path,
        } => {
            run_purge_orphan_institutions(state, dry_run, backup_path.as_deref());
            true
        }
    }
}

#[derive(Debug)]
struct PurgeReport {
    legacy_count: usize,
    private_count: usize,
    citizen_count: usize,
    per_table_deleted: Vec<(&'static str, u64)>,
    dry_run: bool,
}

// 清掉所有不合规 CID 号。PUBLIC 公权机构删后只能从链上唯一真源重建投影;
// PRIVATE 私权机构与公民属注册局直接录入,删后无法自动重建,需由注册局重新录入。
fn run_purge_legacy_cid(state: &AppState, dry_run: bool) {
    let report = state
        .db
        .purge_legacy_cid_rows(dry_run)
        .unwrap_or_else(|e| panic!("purge-legacy-cid failed: {e}"));
    let per_table = report
        .per_table_deleted
        .iter()
        .map(|(table, count)| format!("{table}={count}"))
        .collect::<Vec<_>>()
        .join(", ");
    info!(
        dry_run = report.dry_run,
        legacy_count = report.legacy_count,
        private_permanently_deleted = report.private_count,
        citizen_permanently_deleted = report.citizen_count,
        per_table = %per_table,
        "cid legacy purge finished"
    );
    if report.dry_run {
        info!("purge-legacy-cid dry-run: no rows changed; re-run without --dry-run to apply");
        return;
    }
    if report.legacy_count == 0 {
        info!("purge-legacy-cid: no legacy cid rows; skip chain projection sync");
        return;
    }
    if report.private_count > 0 || report.citizen_count > 0 {
        warn!(
            private_permanently_deleted = report.private_count,
            citizen_permanently_deleted = report.citizen_count,
            "purge-legacy-cid removed user-created PRIVATE/CITIZEN rows; they must be re-created/re-bound to get new-scheme cid"
        );
    }
    init_chain_genesis_hash_blocking()
        .unwrap_or_else(|e| panic!("purge-legacy-cid init chain genesis hash failed: {e}"));
    let projection = crate::indexer::db::government::sync_gov_chain_projection_blocking(&state.db)
        .unwrap_or_else(|e| panic!("purge-legacy-cid chain projection sync failed: {e}"));
    log_gov_projection_report("purge-legacy-cid", &projection);
}

// 一条孤儿机构记录(subjects 行 town_code 指向 china.sqlite 已退役/不存在的镇)。
#[derive(Debug, Clone)]
struct OrphanInstitution {
    province_code: String,
    cid_number: String,
    kind: String,
    city_code: String,
    town_code: String,
    town_name: String,
    category: String,
    institution_code: String,
}

// 清理孤儿机构 CLI。
// `purge-orphan-institutions [--dry-run|--apply] [--backup <path>]`,默认 dry-run。
// 孤儿 = subjects.town_code 非空 + (province_code,city_code,town_code) 不在 china.sqlite 内存树。
//   - dry-run:只打印孤儿清单(onchina/town/town_code/category/institution_code/原因)+ 总数,
//     供人工核对无一命中冻结常量号(储委会/部委)。
//   - apply:先把待删行导出到 purge_orphan_backup_<...>.sql(删除唯一回滚保证),
//     再逐省单事务级联删(accounts→docs→audit→gov|private→ids→subjects)。
// 红线:绝不动 cid_number;绝不删空 town_code 行(已在扫描层白名单过滤);不碰号生成/链/省市码。
fn run_purge_orphan_institutions(state: &AppState, dry_run: bool, backup_path: Option<&str>) {
    let orphans = state
        .db
        .scan_orphan_institutions()
        .unwrap_or_else(|e| panic!("purge-orphan-institutions scan failed: {e}"));

    if orphans.is_empty() {
        info!(
            dry_run,
            "purge-orphan-institutions: no orphan institutions found; nothing to do"
        );
        return;
    }

    // 打印孤儿清单(dry-run 与 apply 都先打,apply 时即删除前留痕)。
    for o in &orphans {
        info!(
            cid_number = %o.cid_number,
            kind = %o.kind,
            province_code = %o.province_code,
            city_code = %o.city_code,
            town_code = %o.town_code,
            town_name = %o.town_name,
            category = %o.category,
            institution_code = %o.institution_code,
            reason = "town (province_code,city_code,town_code) not found in china.sqlite (retired/reused town code)",
            "orphan institution"
        );
    }
    info!(
        dry_run,
        orphan_total = orphans.len(),
        "purge-orphan-institutions scan finished"
    );

    if dry_run {
        info!(
            "purge-orphan-institutions dry-run: no rows changed; review the list above (verify no frozen-constant cid e.g. reserve-committee/ministry is hit) then re-run with --apply to delete"
        );
        return;
    }

    // 按省分组 + 按 kind 拆 gov/private,供逐省事务级联删与备份导出复用。
    let mut gov_by_province: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    let mut private_by_province: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    let mut all_by_province: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for o in &orphans {
        all_by_province
            .entry(o.province_code.clone())
            .or_default()
            .push(o.cid_number.clone());
        match o.kind.as_str() {
            "PRIVATE" => private_by_province
                .entry(o.province_code.clone())
                .or_default()
                .push(o.cid_number.clone()),
            // PUBLIC 及其它(默认按公权机构 gov 表处理)。
            _ => gov_by_province
                .entry(o.province_code.clone())
                .or_default()
                .push(o.cid_number.clone()),
        }
    }

    // 1. 删除前导出待删行(删除唯一回滚保证)。
    let resolved_backup = backup_path.map(str::to_string).unwrap_or_else(|| {
        format!(
            "purge_orphan_backup_{}.sql",
            Utc::now().format("%Y%m%d%H%M%S")
        )
    });
    state
        .db
        .export_orphan_backup(&all_by_province, &resolved_backup)
        .unwrap_or_else(|e| panic!("purge-orphan-institutions backup failed: {e}"));
    info!(
        backup = %resolved_backup,
        provinces = all_by_province.len(),
        orphan_total = orphans.len(),
        "purge-orphan-institutions backup written before delete"
    );

    // 2. 逐省单事务级联删(禁止跨省一条 SQL)。
    let mut deleted_total: u64 = 0;
    for province in all_by_province.keys() {
        let gov_cids = gov_by_province.get(province).cloned().unwrap_or_default();
        let private_cids = private_by_province
            .get(province)
            .cloned()
            .unwrap_or_default();
        let deleted = state
            .db
            .delete_orphan_institutions_by_province(province, &gov_cids, &private_cids)
            .unwrap_or_else(|e| panic!("purge-orphan-institutions delete failed: {e}"));
        info!(
            province_code = %province,
            gov_deleted = gov_cids.len(),
            private_deleted = private_cids.len(),
            subjects_deleted = deleted,
            "purge-orphan-institutions province purged"
        );
        deleted_total += deleted;
    }
    warn!(
        orphan_total = orphans.len(),
        subjects_deleted = deleted_total,
        backup = %resolved_backup,
        "purge-orphan-institutions applied: orphan institutions removed (rollback only via backup file)"
    );
}

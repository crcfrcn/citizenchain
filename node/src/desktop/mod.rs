//! 桌面 GUI 模块（Tauri）。
//!
//! 将 Substrate 区块链节点和 Tauri 桌面界面合并为单一程序。
//! 节点生命周期与 App 进程绑定：
//!
//! - App 启动 → setup 后台线程自动 `start_node_blocking`
//! - 用户关窗（红 X / Cmd+Q / 菜单 Quit / 系统关闭）→ App 退出 →
//!   `RunEvent::Exit` 触发 `cleanup_on_exit`
//! - macOS 黄色横线为系统原生 minimize，不影响节点和进程，无需拦截
//!
//! 三平台（macOS / Windows / Linux）行为统一：关窗即退出软件即停节点。
//! 桌面端各功能模块已扁平化到 crate 根层，例如 `crate::governance` 与 `crate::settings`。

pub(crate) mod node_runner;

use crate::{
    admins, governance,
    home::{self, cleanup_on_exit, cleanup_on_startup, AppState, RuntimeState},
    mining, other, settings,
};
use std::sync::Mutex;
use tauri::Manager;

/// 本机正式构建只消费公开证书；不导出私钥，也不改变其它平台的启动合同。
#[cfg(target_os = "macos")]
const RPC_CERTIFICATE_FILE: &str = "rpc-certificate.hex";

#[cfg(target_os = "macos")]
fn check_rpc_certificate_acl(output: &str, private: bool) -> Result<(), String> {
    let mut lines = output.lines();
    if lines.next().is_none_or(|line| line.is_empty()) {
        return Err("公开证书路径 ACL 无法确认".into());
    }
    for line in lines {
        let (index, entry) = line.trim().split_once(": ")
            .ok_or("公开证书路径 ACL 格式无效")?;
        if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) || private {
            return Err("公开证书目录或文件存在附加 ACL".into());
        }
        if entry.contains(" deny ") { continue; }
        let (_, rights) = entry.split_once(" allow ").ok_or("公开证书路径 ACL 无法确认")?;
        if !rights.split(',').all(|right| matches!(right.trim(),
            "read" | "list" | "execute" | "search" | "readattr" | "readextattr" | "readsecurity" | "synchronize")) {
            return Err("公开证书祖先目录允许额外写入".into());
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn inspect_rpc_certificate_acl(path: &std::path::Path, private: bool) -> Result<(), String> {
    // 使用系统固定工具且不经过 shell；不修复 ACL、不将工具输出写入日志。
    let output = std::process::Command::new("/bin/ls")
        .arg("-lde").arg(path).env_clear().env("LC_ALL", "C").output()
        .map_err(|_| "无法检查公开证书 ACL")?;
    if !output.status.success() || output.stdout.len() > 64 * 1024 {
        return Err("无法检查公开证书 ACL".into());
    }
    check_rpc_certificate_acl(std::str::from_utf8(&output.stdout)
        .map_err(|_| "公开证书 ACL 编码无效")?, private)
}

#[cfg(target_os = "macos")]
fn check_rpc_certificate_directory(directory: &std::path::Path) -> Result<(u64, u64), String> {
    use std::os::unix::fs::MetadataExt;
    let uid = unsafe { libc::geteuid() };
    if !directory.is_absolute() || directory.canonicalize().map_err(|_| "公开证书目录不可用")? != directory {
        return Err("公开证书目录不能经过链接".into());
    }
    for path in directory.ancestors() {
        let metadata = std::fs::symlink_metadata(path).map_err(|_| "公开证书目录不可用")?;
        if !metadata.is_dir() || metadata.file_type().is_symlink()
            || ![0, uid].contains(&metadata.uid()) || metadata.mode() & 0o022 != 0
            || path == directory && (metadata.uid() != uid || metadata.mode() & 0o7777 != 0o700) {
            return Err("公开证书目录所有者或权限无效".into());
        }
        inspect_rpc_certificate_acl(path, path == directory)?;
    }
    let metadata = std::fs::symlink_metadata(directory).map_err(|_| "公开证书目录不可用")?;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(target_os = "macos")]
fn check_rpc_certificate_file(metadata: &std::fs::Metadata) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    if !metadata.is_file() || metadata.file_type().is_symlink()
        || metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o7777 != 0o600
        || metadata.nlink() != 1 || metadata.len() == 0 || metadata.len() > 64 * 1024 {
        return Err("公开证书文件类型、所有者或权限无效".into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn publish_rpc_certificate(directory: &std::path::Path, certificate: &[u8]) -> Result<(), String> {
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    if certificate.is_empty() || certificate.len() > 32 * 1024 {
        return Err("公开证书大小无效".into());
    }
    let identity = check_rpc_certificate_directory(directory)?;
    let path = directory.join(RPC_CERTIFICATE_FILE);
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) => {
            check_rpc_certificate_file(&metadata)?;
            inspect_rpc_certificate_acl(&path, true)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("公开证书文件不可用".into()),
    }
    let text = hex::encode(certificate);
    crate::shared::security::write_text_atomic_restricted(&path, &text)?;
    // 写后仍按句柄核对，避免把路径检查与实际读取分离；同用户恶意进程不在此边界内。
    let mut file = std::fs::OpenOptions::new().read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(&path)
        .map_err(|_| "公开证书回读失败")?;
    let metadata = file.metadata().map_err(|_| "公开证书属性读取失败")?;
    check_rpc_certificate_file(&metadata)?;
    inspect_rpc_certificate_acl(&path, true)?;
    let mut actual = Vec::new();
    file.by_ref().take(64 * 1024 + 1).read_to_end(&mut actual).map_err(|_| "公开证书回读失败")?;
    let current = std::fs::symlink_metadata(&path).map_err(|_| "公开证书回读失败")?;
    check_rpc_certificate_file(&current)?;
    if identity != check_rpc_certificate_directory(directory)?
        || (metadata.dev(), metadata.ino()) != (current.dev(), current.ino())
        || actual != text.as_bytes() {
        return Err("公开证书交付期间文件已变化".into());
    }
    Ok(())
}

/// 启动 Tauri 桌面应用。
///
/// Substrate 节点在 setup 阶段后台线程自动启动；首页仍提供手动启停按钮。
pub fn run_desktop() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState(Mutex::new(RuntimeState::default())))
        .invoke_handler(tauri::generate_handler![
            home::identity::get_node_status,
            home::process::start_node,
            home::process::stop_node,
            home::sync_guard::get_sync_guard_status,
            settings::desktop_update::open_desktop_installer,
            settings::desktop_update::check_desktop_update,
            settings::node_mode::get_node_mode,
            settings::node_mode::set_node_mode,
            settings::onchina_platform::get_onchina_platform,
            settings::onchina_platform::start_onchina_platform,
            settings::onchina_platform::stop_onchina_platform,
            settings::reward_account::get_reward_account,
            settings::reward_account::set_reward_account,
            settings::reward_account::get_local_miner_ss58_address,
            settings::bootnodes_address::get_bootnode_key,
            settings::grandpa_address::get_grandpa_key,
            settings::bootnodes_address::set_bootnode_key,
            settings::grandpa_address::set_grandpa_key,
            settings::bootnodes_address::get_genesis_bootnode_options,
            home::rpc::get_chain_status,
            home::identity::get_node_identity,
            home::rpc::get_total_issuance,
            home::rpc::get_total_stake,
            mining::dashboard::get_mining_dashboard,
            mining::network_overview::get_network_overview,
            other::other_tabs::get_other_tabs_content,
            other::other_tabs::get_whitepaper_document,
            other::other_tabs::get_runtime_constitution_document,
            governance::get_governance_overview,
            governance::get_institution_detail,
            governance::balance_watch::start_governance_balance_watch,
            governance::balance_watch::stop_governance_balance_watch,
            governance::get_proposal_page,
            governance::get_proposal_detail,
            governance::get_next_proposal_id,
            governance::get_institution_proposals,
            governance::get_institution_proposal_page,
            // 双层 ID + 反向索引
            governance::get_proposal_display,
            governance::list_proposals_by_institution,
            governance::list_proposals_by_cid,
            governance::list_proposals_by_owner,
            admins::management::activation::build_activate_admin_request,
            admins::management::activation::verify_activate_admin,
            admins::management::activation::get_activated_admins,
            admins::management::activation::deactivate_admin,
            admins::management::activation::has_any_activated_admin,
            admins::management::commands::get_institution_admins_state,
            admins::management::commands::get_account_balances,
            governance::build_vote_request,
            governance::build_joint_vote_request,
            crate::transaction::multisig::commands::build_multisig_transfer_request,
            crate::transaction::multisig::commands::submit_multisig_transfer,
            crate::transaction::multisig::commands::build_multisig_safety_fund_request,
            crate::transaction::multisig::commands::submit_multisig_safety_fund,
            crate::transaction::multisig::commands::build_multisig_sweep_request,
            crate::transaction::multisig::commands::submit_multisig_sweep,
            governance::runtime_upgrade::commands::get_pow_difficulty_params,
            governance::runtime_upgrade::commands::build_propose_upgrade_request,
            governance::runtime_upgrade::commands::submit_propose_upgrade,
            crate::core::grandpa_rotation::build_grandpa_key_change_request,
            crate::core::grandpa_rotation::submit_grandpa_key_change,
            crate::core::grandpa_rotation::get_grandpa_key_change_status,
            governance::submit_vote,
            governance::check_vote_status,
            crate::transaction::onchain::get_wallets,
            crate::transaction::onchain::add_wallet,
            crate::transaction::onchain::remove_wallet,
            crate::transaction::onchain::set_active_wallet,
            crate::transaction::onchain::get_wallet_balance,
            crate::transaction::onchain::build_cold_transfer_request,
            crate::transaction::onchain::submit_cold_transfer,
            crate::transaction::onchain::submit_hot_transfer,
            // ─── 清算行 offchain tab ───
            crate::transaction::offchain::institution_read::commands::search_eligible_clearing_banks,
            crate::transaction::offchain::commands::query_clearing_bank_node_info,
            crate::transaction::offchain::commands::query_local_peer_id,
            crate::transaction::offchain::commands::test_clearing_bank_endpoint_connectivity,
            crate::transaction::offchain::commands::build_register_clearing_bank_request,
            crate::transaction::offchain::commands::submit_register_clearing_bank,
            crate::transaction::offchain::commands::build_update_clearing_bank_endpoint_request,
            crate::transaction::offchain::commands::submit_update_clearing_bank_endpoint,
            crate::transaction::offchain::commands::build_unregister_clearing_bank_request,
            crate::transaction::offchain::commands::submit_unregister_clearing_bank,
            crate::transaction::offchain::settlement::commands::build_decrypt_admin_request,
            crate::transaction::offchain::settlement::commands::verify_and_decrypt_admin,
            crate::transaction::offchain::settlement::commands::list_decrypted_admins,
            crate::transaction::offchain::settlement::commands::lock_decrypted_admin,
            crate::transaction::offchain::institution_read::commands::fetch_clearing_bank_institution_detail,
            crate::transaction::offchain::institution_read::commands::fetch_clearing_bank_institution_proposals
        ])
        .setup(|app| {
            cleanup_on_startup(app.handle());
            // 先生成本机证书，再让同步守护或节点线程访问 RPC；失败显示在首页。
            let tls_error = crate::core::rpc_tls::prepare_desktop_rpc_tls().err();
            // 同步守护只读本机 RPC，等待节点启动后再按本机状态自检。
            home::sync_guard::start_sync_guard(app.handle().clone());
            crate::core::grandpa_rotation::start_monitor(app.handle().clone());

            if let Some(error) = tls_error {
                let app_state = app.state::<AppState>();
                let mut state = app_state
                    .0
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.node_state = home::process::NodeLifecycleState::Failed;
                state.last_error = Some(error);
                return Ok(());
            }

            // 只增加公开证书交付；失败不改变节点原启停及界面流程，外部脚本保持失败关闭。
            #[cfg(target_os = "macos")]
            if let Err(error) = crate::shared::security::app_data_dir(app.handle()).and_then(|directory| {
                let certificate = crate::core::rpc_tls::desktop_rpc_certificate()
                    .ok_or("桌面 RPC 公开证书尚未准备")?;
                publish_rpc_certificate(&directory, certificate)
            }) {
                eprintln!("[RPC] 公开证书交付失败: {error}");
            }

            // 自动启动节点。在后台线程跑，避免阻塞 setup 让窗口慢出现。
            // start_node_blocking 内部带 5s + 2s 等待，前端通过 get_node_status 轮询自然刷新。
            let app_handle = app.handle().clone();
            if let Err(err) = std::thread::Builder::new()
                .name("auto-start-node".into())
                .spawn(move || {
                    if let Err(e) = home::start_node_blocking(app_handle) {
                        eprintln!("[节点] 自动启动失败: {e}");
                    }
                })
            {
                eprintln!("[节点] 自动启动线程创建失败: {err}");
            }

            Ok(())
        })
        .build(tauri::generate_context!("tauri.conf.json"));
    let app = match app {
        Ok(app) => app,
        Err(err) => {
            eprintln!("启动公民链失败: {err}");
            return;
        }
    };
    app.run(|app, event| {
        if let tauri::RunEvent::Exit = event {
            cleanup_on_exit(app);
            // 如果用户手动启动过链上中国平台,节点退出时一并停掉子进程。
            crate::onchina_proc::stop_onchina();
        }
    });
}

#[cfg(all(test, target_os = "macos"))]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod certificate_tests {
    use super::*;
    use std::{fs, path::PathBuf};
    use std::os::unix::fs::{symlink, PermissionsExt};

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::current_dir().unwrap().join("target")
                .join(format!("rpc-public-{}-{}", std::process::id(), rand::random::<u64>()));
            fs::create_dir_all(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path.canonicalize().unwrap())
        }
    }
    impl Drop for Directory { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

    #[test]
    fn public_certificate_is_atomic_restricted_and_replaced_without_exporting_private_material() {
        let directory = Directory::new();
        let first = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let second = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let path = directory.0.join(RPC_CERTIFICATE_FILE);
        publish_rpc_certificate(&directory.0, first.cert.der()).unwrap();
        check_rpc_certificate_file(&fs::symlink_metadata(&path).unwrap()).unwrap();
        assert_eq!(hex::decode(fs::read_to_string(&path).unwrap()).unwrap(), first.cert.der().as_ref());
        publish_rpc_certificate(&directory.0, second.cert.der()).unwrap();
        assert_eq!(hex::decode(fs::read_to_string(&path).unwrap()).unwrap(), second.cert.der().as_ref());
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[test]
    fn public_certificate_rejects_links_types_permissions_and_invalid_size() {
        let directory = Directory::new();
        let credentials = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let cert = credentials.cert.der();
        let path = directory.0.join(RPC_CERTIFICATE_FILE);
        let other = directory.0.join("other");
        fs::write(&other, b"unchanged").unwrap();
        symlink(&other, &path).unwrap();
        assert!(publish_rpc_certificate(&directory.0, cert).is_err());
        assert_eq!(fs::read(&other).unwrap(), b"unchanged");
        fs::remove_file(&path).unwrap();
        fs::hard_link(&other, &path).unwrap();
        assert!(publish_rpc_certificate(&directory.0, cert).is_err());
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(publish_rpc_certificate(&directory.0, cert).is_err());
        fs::remove_dir(&path).unwrap();
        publish_rpc_certificate(&directory.0, cert).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(publish_rpc_certificate(&directory.0, cert).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(publish_rpc_certificate(&directory.0, cert).is_err());
        fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(publish_rpc_certificate(&directory.0, &[]).is_err());
        assert!(publish_rpc_certificate(&directory.0, &vec![0; 32 * 1024 + 1]).is_err());
        assert!(publish_rpc_certificate(&directory.0.join("missing"), cert).is_err());
        let linked = directory.0.join("linked"); symlink(&directory.0, &linked).unwrap();
        assert!(publish_rpc_certificate(&linked, cert).is_err());
    }

    #[test]
    fn public_certificate_acl_is_closed_and_ancestor_write_grants_are_rejected() {
        assert!(check_rpc_certificate_acl("directory\n", true).is_ok());
        assert!(check_rpc_certificate_acl("directory\n 0: group:everyone deny delete\n", false).is_ok());
        assert!(check_rpc_certificate_acl("directory\n 0: user:fixture allow read,search\n", false).is_ok());
        for (output, private) in [("", false), ("directory\nunknown\n", false),
            ("directory\n 0: user:fixture allow write\n", false),
            ("directory\n 0: group:everyone deny delete\n", true)] {
            assert!(check_rpc_certificate_acl(output, private).is_err());
        }
    }
}

use crate::lifecycle::cleanup_on_exit;
use crate::lifecycle::cleanup_on_startup;
use nodecontrol::state::AppState;
#[cfg(target_os = "macos")]
use nodedesktop::publication::publish_desktop_rpc_certificate;
use std::sync::Mutex;
use tauri::Manager;

/// 启动 Tauri 桌面应用。
///
/// Substrate 节点在 setup 阶段后台线程自动启动；首页仍提供手动启停按钮。
pub fn run_desktop() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState(Mutex::new(
            nodecontrol::state::RuntimeState::default(),
        )))
        .invoke_handler(tauri::generate_handler![
            nodecontrol::identity::get_node_status,
            crate::lifecycle::start_node,
            crate::lifecycle::stop_node,
            nodecontrol::sync::get_sync_guard_status,
            nodecontrol::update::download_desktop_update,
            nodecontrol::update::desktop_update_status,
            nodecontrol::update::install_desktop_update,
            nodecontrol::update::check_desktop_update,
            nodecontrol::mode::get_node_mode,
            nodecontrol::mode::set_node_mode,
            crate::platform::get_onchina_platform,
            crate::platform::start_onchina_platform,
            crate::platform::stop_onchina_platform,
            nodecontrol::reward::get_reward_account,
            nodecontrol::reward::set_reward_account,
            nodecontrol::reward::get_local_miner_ss58_address,
            nodecontrol::bootnodes::get_bootnode_key,
            nodecontrol::grandpa::get_grandpa_key,
            crate::bootnodes::set_bootnode_key,
            crate::grandpa::set_grandpa_key,
            nodecontrol::bootnodes::get_genesis_bootnode_options,
            nodecontrol::rpc::get_chain_status,
            nodecontrol::identity::get_node_identity,
            nodecontrol::rpc::get_total_issuance,
            nodecontrol::rpc::get_total_stake,
            nodemining::dashboard::get_mining_dashboard,
            nodemining::network::get_network_overview,
            nodecontrol::documents::get_other_tabs_content,
            nodecontrol::documents::get_whitepaper_document,
            nodecontrol::documents::get_runtime_constitution_document,
            nodeinstitution::council::get_governance_overview,
            nodeinstitution::council::get_institution_detail,
            nodeinstitution::watch::start_governance_balance_watch,
            nodeinstitution::watch::stop_governance_balance_watch,
            nodeproposal::commands::get_proposal_page,
            nodeproposal::commands::get_proposal_detail,
            nodeproposal::commands::get_next_proposal_id,
            nodeproposal::commands::get_institution_proposals,
            nodeproposal::commands::get_institution_proposal_page,
            // 双层 ID + 反向索引
            nodeproposal::commands::get_proposal_display,
            nodeproposal::commands::list_proposals_by_institution,
            nodeproposal::commands::list_proposals_by_cid,
            nodeproposal::commands::list_proposals_by_owner,
            nodeinstitution::activation::build_activate_admin_request,
            nodeinstitution::activation::verify_activate_admin,
            nodeinstitution::activation::get_activated_admins,
            nodeinstitution::activation::deactivate_admin,
            nodeinstitution::activation::has_any_activated_admin,
            nodeinstitution::commands::get_institution_admins_state,
            nodeinstitution::commands::get_account_balances,
            nodeproposal::commands::build_vote_request,
            nodeproposal::commands::build_joint_vote_request,
            nodemultisig::commands::build_multisig_transfer_request,
            nodemultisig::commands::submit_multisig_transfer,
            nodemultisig::commands::build_multisig_safety_fund_request,
            nodemultisig::commands::submit_multisig_safety_fund,
            nodemultisig::commands::build_multisig_sweep_request,
            nodemultisig::commands::submit_multisig_sweep,
            nodeupgrade::commands::get_pow_difficulty_params,
            nodeupgrade::commands::build_propose_upgrade_request,
            nodeupgrade::commands::submit_propose_upgrade,
            nodegrandpakey::commands::build_grandpa_key_change_request,
            nodegrandpakey::commands::submit_grandpa_key_change,
            nodegrandpakey::commands::get_grandpa_key_change_status,
            nodeproposal::commands::submit_vote,
            nodeproposal::commands::check_vote_status,
            nodeonchain::commands::get_wallets,
            nodeonchain::commands::add_wallet,
            nodeonchain::commands::remove_wallet,
            nodeonchain::commands::set_active_wallet,
            nodeonchain::commands::get_wallet_balance,
            nodeonchain::commands::build_cold_transfer_request,
            nodeonchain::commands::submit_cold_transfer,
            nodeonchain::commands::submit_hot_transfer,
            // ─── 清算行 offchain tab ───
            nodeinstitution::commands::search_eligible_clearing_banks,
            nodeoffchain::commands::query_clearing_bank_node_info,
            nodeoffchain::commands::query_local_peer_id,
            nodeoffchain::commands::test_clearing_bank_endpoint_connectivity,
            nodeoffchain::commands::build_register_clearing_bank_request,
            nodeoffchain::commands::submit_register_clearing_bank,
            nodeoffchain::commands::build_update_clearing_bank_endpoint_request,
            nodeoffchain::commands::submit_update_clearing_bank_endpoint,
            nodeoffchain::commands::build_unregister_clearing_bank_request,
            nodeoffchain::commands::submit_unregister_clearing_bank,
            nodeoffchain::commands::build_decrypt_admin_request,
            nodeoffchain::commands::verify_and_decrypt_admin,
            nodeoffchain::commands::list_decrypted_admins,
            nodeoffchain::commands::lock_decrypted_admin,
            nodeinstitution::commands::fetch_clearing_bank_institution_detail,
            nodeinstitution::commands::fetch_clearing_bank_institution_proposals
        ])
        .setup(|app| {
            cleanup_on_startup(app.handle());
            // 先生成本机证书，再让同步守护或节点线程访问 RPC；失败显示在首页。
            let tls_error = nodedesktop::tls::prepare_desktop_rpc_tls().err();
            // 同步守护只读本机 RPC，等待节点启动后再按本机状态自检。
            nodecontrol::sync::start_sync_guard(app.handle().clone());
            crate::rotation::start_monitor(app.handle().clone());

            if let Some(error) = tls_error {
                let app_state = app.state::<AppState>();
                let mut state = app_state
                    .0
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.node_state = nodecontrol::state::NodeLifecycleState::Failed;
                state.last_error = Some(error);
                return Ok(());
            }

            // 只增加公开证书交付；失败不改变节点原启停及界面流程，外部脚本保持失败关闭。
            #[cfg(target_os = "macos")]
            if let Err(error) = publish_desktop_rpc_certificate(app.handle()) {
                eprintln!("[RPC] 公开证书交付失败: {error}");
            }

            // 自动启动节点。在后台线程跑，避免阻塞 setup 让窗口慢出现。
            // start_node_blocking 内部带 5s + 2s 等待，前端通过 get_node_status 轮询自然刷新。
            let app_handle = app.handle().clone();
            if let Err(err) = std::thread::Builder::new()
                .name("auto-start-node".into())
                .spawn(move || {
                    if let Err(e) = crate::lifecycle::start_node_blocking(app_handle) {
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
            crate::platform::stop_onchina();
        }
    });
}

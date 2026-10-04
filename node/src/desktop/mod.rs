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

/// 启动 Tauri 桌面应用。
///
/// Substrate 节点在 setup 阶段后台线程自动启动；首页仍提供手动启停按钮。
pub fn run_desktop() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState(Mutex::new(RuntimeState::default())))
        .invoke_handler(tauri::generate_handler![
            home::identity::get_node_status,
            home::process::start_node,
            home::process::stop_node,
            home::sync_guard::get_sync_guard_status,
            settings::desktop_update::prepare_desktop_update,
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
            // 同步守护只读本机 RPC，等待节点启动后再按本机状态自检。
            home::sync_guard::start_sync_guard(app.handle().clone());
            crate::core::grandpa_rotation::start_monitor(app.handle().clone());

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

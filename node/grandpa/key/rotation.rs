//! 监视GRANDPA密钥更换状态并协调本机轮换。

use crate::key::proof::decode_hex_32;
use crate::key::proof::fetch_authorities;
use crate::key::proof::fetch_current_public_key;
use crate::key::proof::finalized_context;
use crate::key::storage::clear_pending;
use crate::key::storage::load_pending;

use std::thread;
use std::time::Duration;
use tauri::AppHandle;

const MONITOR_INTERVAL: Duration = Duration::from_secs(5);

fn monitor_once(app: &AppHandle) -> Result<(), String> {
    let Some(pending) = load_pending(app)? else {
        return Ok(());
    };
    let (finalized_hash, finalized_number) = finalized_context()?;
    let old_public_key = decode_hex_32(&pending.old_public_key, "old_public_key")?;
    let new_public_key = decode_hex_32(&pending.new_public_key, "new_public_key")?;
    let current_public_key = fetch_current_public_key(&pending.actor_cid_number, &finalized_hash)?;
    let authorities = fetch_authorities(&finalized_hash)?;
    let old_authority =
        sp_consensus_grandpa::AuthorityId::from(sp_core::ed25519::Public::from_raw(old_public_key));
    let new_authority =
        sp_consensus_grandpa::AuthorityId::from(sp_core::ed25519::Public::from_raw(new_public_key));
    let old_is_active = authorities
        .iter()
        .any(|(authority, _)| authority == &old_authority);
    let new_is_active = authorities
        .iter()
        .any(|(authority, _)| authority == &new_authority);

    match next_action(
        current_public_key == new_public_key,
        new_is_active,
        old_is_active,
        pending.tx_hash.is_some(),
        finalized_number,
        pending.proof_expires_at,
    ) {
        Action::Finalize => {
            crate::key::local::finalize_rotation_key(
                app,
                &pending.cid_short_name,
                &old_public_key,
                &new_public_key,
            )?;
            clear_pending(app)?;
            let host = crate::key::Host::get(app)?;
            if (host.is_running)(app)? {
                (host.stop)(app.clone())?;
                (host.start)(app.clone())?;
            }
            eprintln!(
                "[GRANDPA 换钥] finalized 已确认新 authority 0x{}，旧私钥已自动删除",
                hex::encode(new_public_key)
            );
        }
        Action::Discard => {
            // 用户未提交且证明窗口已过期时清理候选新私钥；已提交的内部投票可能持续更久，
            // 不能按证明提交期限误删仍待治理通过的新私钥。
            crate::key::local::discard_rotation_candidate(app, &new_public_key)?;
            clear_pending(app)?;
        }
        Action::Wait => {}
    }
    Ok(())
}

/// 桌面应用启动后持续监视 finalized 状态，确保重启后仍能完成旧私钥清理。
pub fn start_monitor(app: AppHandle) {
    if let Err(error) = thread::Builder::new()
        .name("grandpa-key-rotation-monitor".into())
        .spawn(move || loop {
            if let Err(error) = monitor_once(&app) {
                eprintln!("[GRANDPA 换钥] finalized 监视暂时失败: {error}");
            }
            thread::sleep(MONITOR_INTERVAL);
        })
    {
        eprintln!("[GRANDPA 换钥] 启动 finalized 监视线程失败: {error}");
    }
}

/// 原最终性监视器的三种收尾状态。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Finalize,
    Discard,
    Wait,
}

/// 新公钥登记与权威集同时生效才切换；已提交治理不会按持钥证明期限误清理。
pub(crate) fn next_action(
    current_matches: bool,
    new_is_active: bool,
    old_is_active: bool,
    submitted: bool,
    finalized_number: u64,
    proof_expires_at: u32,
) -> Action {
    if current_matches && new_is_active && !old_is_active {
        Action::Finalize
    } else if !submitted && finalized_number > u64::from(proof_expires_at) {
        Action::Discard
    } else {
        Action::Wait
    }
}

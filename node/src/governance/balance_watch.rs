// 治理余额 watcher：监听详情页当前机构，在新区块 finalized 后刷新链上真实金额。

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::Duration,
};

use tauri::{AppHandle, Emitter};

use super::{build_institution_balance_update_sync, registry};

const BALANCE_WATCH_EVENT: &str = "governance-balance-updated";
const BALANCE_WATCH_INTERVAL: Duration = Duration::from_secs(1);

static ACTIVE_WATCHES: LazyLock<Mutex<HashMap<String, u64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn register_watch(cid_number: &str) -> Result<u64, String> {
    let mut watches = ACTIVE_WATCHES
        .lock()
        .map_err(|_| "治理余额 watcher 状态锁获取失败".to_string())?;
    let generation = watches.get(cid_number).copied().unwrap_or(0) + 1;
    watches.insert(cid_number.to_string(), generation);
    Ok(generation)
}

fn unregister_watch(cid_number: &str) -> Result<(), String> {
    let mut watches = ACTIVE_WATCHES
        .lock()
        .map_err(|_| "治理余额 watcher 状态锁获取失败".to_string())?;
    watches.remove(cid_number);
    Ok(())
}

fn is_watch_active(cid_number: &str, generation: u64) -> Result<bool, String> {
    let watches = ACTIVE_WATCHES
        .lock()
        .map_err(|_| "治理余额 watcher 状态锁获取失败".to_string())?;
    Ok(watches.get(cid_number).copied() == Some(generation))
}

#[tauri::command]
pub async fn start_governance_balance_watch(
    app: AppHandle,
    cid_number: String,
) -> Result<(), String> {
    if registry::find_institution(&cid_number).is_none() {
        return Err(format!("未知的机构 cidNumber: {cid_number}"));
    }

    let generation = register_watch(&cid_number)?;
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut last_payload_json: Option<String> = None;

        loop {
            match is_watch_active(&cid_number, generation) {
                Ok(true) => {}
                Ok(false) => break,
                Err(_) => break,
            }

            let payload = tauri::async_runtime::spawn_blocking({
                let app_handle = app_handle.clone();
                let cid_number = cid_number.clone();
                move || build_institution_balance_update_sync(&app_handle, &cid_number)
            })
            .await;

            if let Ok(Ok(payload)) = payload {
                if let Ok(payload_json) = serde_json::to_string(&payload) {
                    if last_payload_json.as_deref() != Some(payload_json.as_str()) {
                        let _ = app_handle.emit(BALANCE_WATCH_EVENT, &payload);
                        last_payload_json = Some(payload_json);
                    }
                }
            }

            tokio::time::sleep(BALANCE_WATCH_INTERVAL).await;
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn stop_governance_balance_watch(cid_number: String) -> Result<(), String> {
    unregister_watch(&cid_number)
}

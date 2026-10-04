//! 本机同步守护：检测底层 P2P 已连接但 block sync peer 表为空的脱钩状态。
//!
//! 守护器只访问本机 `127.0.0.1` RPC，不定时请求公网参考节点，也不把区块高度
//! 是否增长作为故障条件。检测到脱钩后只进入降级状态，不做进程内自动重启，
//! 避免 Substrate/RocksDB 释放滞后时把本机节点带入同进程 LOCK 占用状态。

use crate::shared::{constants::RPC_RESPONSE_LIMIT_LARGE, rpc, security};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::VecDeque,
    sync::{mpsc, Mutex, OnceLock},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::AppHandle;

use super::identity::current_status;

const CHECK_INTERVAL: Duration = Duration::from_secs(30);
const STARTUP_GRACE: Duration = Duration::from_secs(120);
const REQUIRED_SUSPECT_SAMPLES: u32 = 6;
const DEGRADED_AUDIT_WINDOW: Duration = Duration::from_secs(10 * 60);

// 守护线程与前端状态分别持有独立互斥量，避免诊断读取阻塞停止信号。
static GUARD_RUNTIME: OnceLock<Mutex<Option<SyncGuardRuntime>>> = OnceLock::new();
static GUARD_STATUS: OnceLock<Mutex<SyncGuardStatus>> = OnceLock::new();

struct SyncGuardRuntime {
    stop_tx: mpsc::Sender<()>,
    thread: JoinHandle<()>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// 前端/诊断读取的同步守护状态。
pub struct SyncGuardStatus {
    pub running: bool,
    pub state: String,
    pub consecutive_suspects: u32,
    pub degraded_count_in_window: usize,
    pub degraded: bool,
    pub last_reason: Option<String>,
    pub last_updated_unix_secs: Option<u64>,
}

impl Default for SyncGuardStatus {
    fn default() -> Self {
        Self {
            running: false,
            state: "stopped".to_string(),
            consecutive_suspects: 0,
            degraded_count_in_window: 0,
            degraded: false,
            last_reason: None,
            last_updated_unix_secs: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct LocalSyncSample {
    should_have_peers: bool,
    health_peers: usize,
    is_syncing: bool,
    system_peers: usize,
    raw_connected_peers: usize,
    raw_identified_peers: usize,
}

fn guard_runtime() -> &'static Mutex<Option<SyncGuardRuntime>> {
    GUARD_RUNTIME.get_or_init(|| Mutex::new(None))
}

fn guard_status() -> &'static Mutex<SyncGuardStatus> {
    GUARD_STATUS.get_or_init(|| Mutex::new(SyncGuardStatus::default()))
}

fn unix_secs_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn update_status<F>(f: F)
where
    F: FnOnce(&mut SyncGuardStatus),
{
    let mut status = guard_status().lock().unwrap_or_else(|err| err.into_inner());
    f(&mut status);
    status.last_updated_unix_secs = Some(unix_secs_now());
}

fn rpc_post_local(method: &str, params: Value) -> Result<Value, String> {
    rpc::rpc_post(
        method,
        params,
        rpc::RPC_REQUEST_TIMEOUT,
        RPC_RESPONSE_LIMIT_LARGE,
    )
}

fn parse_bool(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(v)) => *v,
        Some(Value::String(v)) => v.trim().eq_ignore_ascii_case("true"),
        _ => false,
    }
}

fn sample_local_sync() -> Result<LocalSyncSample, String> {
    let health = rpc_post_local("system_health", Value::Array(vec![]))?;
    let peers = rpc_post_local("system_peers", Value::Array(vec![]))?;
    let network_state = rpc_post_local("system_unstable_networkState", Value::Array(vec![]))?;

    let health_peers = health.get("peers").and_then(Value::as_u64).unwrap_or(0) as usize;
    let should_have_peers = parse_bool(health.get("shouldHavePeers"));
    let is_syncing = parse_bool(health.get("isSyncing"));
    let system_peers = peers.as_array().map(Vec::len).unwrap_or(0);

    let connected = network_state
        .get("connectedPeers")
        .and_then(Value::as_object);
    let raw_connected_peers = connected.map(|items| items.len()).unwrap_or(0);
    let raw_identified_peers = connected
        .map(|items| {
            items
                .values()
                .filter(|peer| {
                    let has_version = peer
                        .get("versionString")
                        .and_then(Value::as_str)
                        .map(|v| !v.trim().is_empty())
                        .unwrap_or(false);
                    let has_ping = peer
                        .get("latestPingTime")
                        .map(|v| !v.is_null())
                        .unwrap_or(false);
                    has_version && has_ping
                })
                .count()
        })
        .unwrap_or(0);

    Ok(LocalSyncSample {
        should_have_peers,
        health_peers,
        is_syncing,
        system_peers,
        raw_connected_peers,
        raw_identified_peers,
    })
}

fn is_sync_network_detached(sample: &LocalSyncSample) -> bool {
    sample.should_have_peers
        && !sample.is_syncing
        && sample.health_peers == 0
        && sample.system_peers == 0
        && sample.raw_connected_peers > 0
        && sample.raw_identified_peers > 0
}

fn sample_reason(sample: &LocalSyncSample) -> String {
    format!(
        "healthPeers={}, systemPeers={}, rawConnected={}, rawIdentified={}, isSyncing={}, shouldHavePeers={}",
        sample.health_peers,
        sample.system_peers,
        sample.raw_connected_peers,
        sample.raw_identified_peers,
        sample.is_syncing,
        sample.should_have_peers,
    )
}

fn prune_degraded_window(events: &mut VecDeque<Instant>, now: Instant) {
    while let Some(front) = events.front() {
        if now.duration_since(*front) <= DEGRADED_AUDIT_WINDOW {
            break;
        }
        events.pop_front();
    }
}

fn append_guard_audit(app: &AppHandle, status: &str) {
    if let Err(err) = security::append_audit_log(app, "sync_guard", status) {
        eprintln!("[sync_guard] 审计日志写入失败: {err}");
    }
}

fn guard_loop(app: AppHandle, stop_rx: mpsc::Receiver<()>) {
    let mut first_running_seen: Option<Instant> = None;
    let mut consecutive_suspects = 0u32;
    let mut degraded_times: VecDeque<Instant> = VecDeque::new();
    let mut degraded = false;

    loop {
        if stop_rx.recv_timeout(CHECK_INTERVAL).is_ok() {
            update_status(|status| {
                status.running = false;
                status.state = "stopped".to_string();
                status.last_reason = Some("sync guard stopped".to_string());
            });
            return;
        }

        let running = current_status(&app)
            .map(|status| status.running)
            .unwrap_or(false);
        if !running {
            first_running_seen = None;
            consecutive_suspects = 0;
            degraded = false;
            update_status(|status| {
                status.running = true;
                status.state = "waiting_node".to_string();
                status.consecutive_suspects = 0;
                status.degraded = false;
                status.degraded_count_in_window = degraded_times.len();
                status.last_reason = Some("node is not running".to_string());
            });
            continue;
        }

        let started_at = *first_running_seen.get_or_insert_with(Instant::now);
        if started_at.elapsed() < STARTUP_GRACE {
            update_status(|status| {
                status.running = true;
                status.state = "warming_up".to_string();
                status.consecutive_suspects = 0;
                status.degraded = degraded;
                status.degraded_count_in_window = degraded_times.len();
                status.last_reason = Some("node startup grace window".to_string());
            });
            continue;
        }

        let now = Instant::now();
        prune_degraded_window(&mut degraded_times, now);

        let sample = match sample_local_sync() {
            Ok(sample) => sample,
            Err(err) => {
                consecutive_suspects = 0;
                update_status(|status| {
                    status.running = true;
                    status.state = "sample_failed".to_string();
                    status.consecutive_suspects = 0;
                    status.degraded = degraded;
                    status.degraded_count_in_window = degraded_times.len();
                    status.last_reason = Some(err);
                });
                continue;
            }
        };

        let reason = sample_reason(&sample);
        if !is_sync_network_detached(&sample) {
            consecutive_suspects = 0;
            degraded = false;
            update_status(|status| {
                status.running = true;
                status.state = "healthy".to_string();
                status.consecutive_suspects = 0;
                status.degraded = degraded;
                status.degraded_count_in_window = degraded_times.len();
                status.last_reason = Some(reason.clone());
            });
            continue;
        }

        consecutive_suspects = consecutive_suspects.saturating_add(1);
        update_status(|status| {
            status.running = true;
            status.state = "suspect".to_string();
            status.consecutive_suspects = consecutive_suspects;
            status.degraded = degraded;
            status.degraded_count_in_window = degraded_times.len();
            status.last_reason = Some(reason.clone());
        });

        if consecutive_suspects < REQUIRED_SUSPECT_SAMPLES {
            continue;
        }

        if !degraded {
            append_guard_audit(&app, "degraded");
            degraded_times.push_back(now);
        }
        degraded = true;
        update_status(|status| {
            status.running = true;
            status.state = "degraded".to_string();
            status.consecutive_suspects = consecutive_suspects;
            status.degraded_count_in_window = degraded_times.len();
            status.degraded = true;
            status.last_reason = Some(format!(
                "{reason}; auto restart disabled to avoid RocksDB lock reuse"
            ));
        });
    }
}

/// 启动同步守护线程。重复调用会被忽略。
pub(crate) fn start_sync_guard(app: AppHandle) {
    let mut runtime = guard_runtime()
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    if runtime.is_some() {
        return;
    }

    let (stop_tx, stop_rx) = mpsc::channel();
    let thread = match thread::Builder::new()
        .name("sync-guard".to_string())
        .spawn(move || guard_loop(app, stop_rx))
    {
        Ok(thread) => thread,
        Err(err) => {
            update_status(|status| {
                status.running = false;
                status.state = "start_failed".to_string();
                status.last_reason = Some(format!("同步守护线程启动失败: {err}"));
                status.last_updated_unix_secs = None;
            });
            return;
        }
    };

    *runtime = Some(SyncGuardRuntime { stop_tx, thread });
    update_status(|status| {
        status.running = true;
        status.state = "started".to_string();
        status.last_reason = Some("sync guard started".to_string());
    });
}

/// 停止同步守护线程，App 退出时必须先停守护器再停节点。
pub(crate) fn stop_sync_guard() {
    let runtime = {
        let mut guard = guard_runtime()
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        guard.take()
    };
    if let Some(runtime) = runtime {
        let _ = runtime.stop_tx.send(());
        let _ = runtime.thread.join();
    }
}

fn status_snapshot() -> SyncGuardStatus {
    guard_status()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .clone()
}

#[tauri::command]
pub async fn get_sync_guard_status() -> Result<SyncGuardStatus, String> {
    Ok(status_snapshot())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suspect_sample() -> LocalSyncSample {
        LocalSyncSample {
            should_have_peers: true,
            health_peers: 0,
            is_syncing: false,
            system_peers: 0,
            raw_connected_peers: 1,
            raw_identified_peers: 1,
        }
    }

    #[test]
    fn detects_sync_network_detached_without_block_height() {
        assert!(is_sync_network_detached(&suspect_sample()));
    }

    #[test]
    fn does_not_degrade_when_raw_network_is_disconnected() {
        let sample = LocalSyncSample {
            raw_connected_peers: 0,
            raw_identified_peers: 0,
            ..suspect_sample()
        };
        assert!(!is_sync_network_detached(&sample));
    }

    #[test]
    fn does_not_degrade_when_sync_peers_exist() {
        let sample = LocalSyncSample {
            health_peers: 1,
            system_peers: 1,
            ..suspect_sample()
        };
        assert!(!is_sync_network_detached(&sample));
    }

    #[test]
    fn does_not_degrade_while_major_syncing() {
        let sample = LocalSyncSample {
            is_syncing: true,
            ..suspect_sample()
        };
        assert!(!is_sync_network_detached(&sample));
    }

    #[test]
    fn does_not_degrade_without_identified_raw_peer() {
        let sample = LocalSyncSample {
            raw_identified_peers: 0,
            ..suspect_sample()
        };
        assert!(!is_sync_network_detached(&sample));
    }

    #[test]
    fn degraded_window_prunes_old_entries() {
        let now = Instant::now();
        let mut events = VecDeque::from([
            now - DEGRADED_AUDIT_WINDOW - Duration::from_secs(1),
            now - Duration::from_secs(1),
        ]);
        prune_degraded_window(&mut events, now);
        assert_eq!(events.len(), 1);
    }
}

use crate::network::LocalNetworkReader;

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread::JoinHandle;

use tokio::sync::oneshot;

/// 节点运行句柄。drop 时会通知后台线程退出并 join。
pub struct NodeHandle {
    pub shutdown_tx: Option<oneshot::Sender<()>>,
    pub thread: Option<JoinHandle<()>>,
    pub alive: Arc<AtomicBool>,
    pub exit_error: Arc<Mutex<Option<String>>>,
    pub network: LocalNetworkReader,
}

impl NodeHandle {
    pub fn network_reader(&self) -> LocalNetworkReader {
        self.network.clone()
    }
    /// 返回后台 Substrate 线程是否仍存活，用于把异常退出同步到首页状态。
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    /// 取出后台必要任务退出时保留的真实原因。
    pub fn take_exit_error(&self) -> Option<String> {
        self.exit_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }
}

impl Drop for NodeHandle {
    fn drop(&mut self) {
        self.network.close();
        // 发送停机信号；接收端可能已被 task_manager 退出路径丢弃，忽略错误。
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        // join 线程，等待 task_manager + tokio runtime 完整 drop，
        // 确保 Substrate Backend 释放 RocksDB LOCK 后再返回。
        if let Some(handle) = self.thread.take() {
            if let Err(err) = handle.join() {
                eprintln!("[节点] Substrate 后台线程退出异常: {err:?}");
            }
        }
        self.alive.store(false, Ordering::Release);
    }
}

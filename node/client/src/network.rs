//! 本机原生RPC的只读会话与固定网络查询。

// Substrate service API 固定返回 sc_service::Error，Node 必须保持框架函数签名。
#![allow(clippy::result_large_err)]

use futures::FutureExt;
use runtime;

use sc_network::NetworkBackend as _;

use sp_keystore::Keystore;
use sp_runtime::traits::Block as BlockT;

use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

/// 每次启动独有的桌面网络只读入口；不接受来自前端的方法名或参数。
#[derive(Clone, Default)]
pub struct LocalNetworkReader(Arc<Mutex<LocalNetworkSession>>);

#[derive(Default)]
struct LocalNetworkSession {
    closed: bool,
    rpc: Option<(Arc<jsonrpsee::RpcModule<()>>, tokio::runtime::Handle)>,
}

#[derive(Clone, Copy)]
pub(crate) enum NetworkRead {
    Health,
    Peers,
    State,
}

impl NetworkRead {
    fn method(self) -> &'static str {
        match self {
            Self::Health => "system_health",
            Self::Peers => "system_peers",
            Self::State => "system_unstable_networkState",
        }
    }
}

pub struct LocalNetworkSnapshot {
    pub health: serde_json::Value,
    pub peers: serde_json::Value,
    pub network_state: serde_json::Value,
}

impl LocalNetworkReader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn attach(
        &self,
        rpc: Arc<jsonrpsee::RpcModule<()>>,
        runtime: tokio::runtime::Handle,
    ) -> Result<(), String> {
        let mut session = self.0.lock().map_err(|_| "本机网络查询状态锁异常")?;
        if session.closed || session.rpc.is_some() {
            return Err("本机网络查询入口已关闭或已经绑定".into());
        }
        session.rpc = Some((rpc, runtime));
        Ok(())
    }

    pub fn close(&self) {
        let mut session = self.0.lock().unwrap_or_else(|error| error.into_inner());
        session.closed = true;
        session.rpc = None;
    }

    fn read(&self, query: NetworkRead) -> Result<serde_json::Value, String> {
        let (rpc, runtime) = {
            let session = self.0.lock().map_err(|_| "本机网络查询状态锁异常")?;
            if session.closed {
                return Err("本机节点网络查询入口已关闭".into());
            }
            session.rpc.clone().ok_or("本机节点网络查询入口尚未就绪")?
        };
        let method = query.method();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        // 在节点自己的 Runtime 执行，调用方不持有节点状态锁，也不嵌套 block_on。
        runtime.spawn(async move {
            let result = tokio::time::timeout(
                Duration::from_secs(4),
                rpc.call::<_, serde_json::Value>(method, Vec::<serde_json::Value>::new()),
            )
            .await
            .map_err(|_| format!("本机 {method} 查询超时"))
            .and_then(|result| result.map_err(|error| format!("本机 {method} 查询失败: {error}")));
            let _ = sender.send(result);
        });
        let result = receiver
            .recv_timeout(Duration::from_secs(5))
            .map_err(|error| format!("本机 {method} 查询未完成: {error}"))?;
        // 停机期间返回的旧响应不能被交给新一轮界面或同步诊断。
        if self.0.lock().map_err(|_| "本机网络查询状态锁异常")?.closed {
            return Err("本机节点网络查询入口已关闭".into());
        }
        let value = result?;
        let valid = match query {
            crate::network::NetworkRead::Peers => value.is_array(),
            crate::network::NetworkRead::Health | crate::network::NetworkRead::State => {
                value.is_object()
            }
        };
        if !valid {
            return Err(format!("本机 {method} 返回格式无效"));
        }
        Ok(value)
    }

    pub fn peers(&self) -> Result<serde_json::Value, String> {
        self.read(crate::network::NetworkRead::Peers)
    }

    pub fn sync_snapshot(&self) -> Result<LocalNetworkSnapshot, String> {
        Ok(LocalNetworkSnapshot {
            health: self.read(crate::network::NetworkRead::Health)?,
            peers: self.peers()?,
            network_state: self.read(crate::network::NetworkRead::State)?,
        })
    }
}

#[cfg(test)]
#[path = "../tests/integration.rs"]
mod tests;

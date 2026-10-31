//! mDNS 服务广告:把本节点 onchina 控制台广告为 `onchina.local`,
//! 办公室局域网内任意机器浏览器输 `https://onchina.local:<port>` 即解析到本节点。
//!
//! 全 best-effort:注册失败只 warn,绝不阻塞 HTTPS 服务。daemon 须长驻才能持续应答 mDNS 查询。

/// onchina 控制台 mDNS 服务类型。
const SERVICE_TYPE: &str = "_onchina._tcp.local.";
const FIXED_HOST_LABEL: &str = "onchina";

/// 后台广告 onchina.local mDNS 服务;长驻线程持有 daemon 守住广告。
pub(crate) fn advertise(port: u16) {
    std::thread::spawn(move || {
        if let Err(err) = run_advertise(FIXED_HOST_LABEL, port) {
            tracing::warn!(error = %err, "mDNS advertise failed; LAN onchina.local unavailable");
        }
    });
}

#[path = "worker.rs"]
mod worker;
use worker::run_advertise;

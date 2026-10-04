//! mDNS 服务广告:把本节点 onchina 控制台广告为 `onchina.local`,
//! 办公室局域网内任意机器浏览器输 `https://onchina.local:<port>` 即解析到本节点。
//!
//! 全 best-effort:注册失败只 warn,绝不阻塞 HTTPS 服务。daemon 须长驻才能持续应答 mDNS 查询。

use mdns_sd::{ServiceDaemon, ServiceInfo};

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

fn run_advertise(host_label: &str, port: u16) -> Result<(), String> {
    let daemon = ServiceDaemon::new().map_err(|e| format!("mdns daemon: {e}"))?;
    let host_name = format!("{host_label}.local.");
    // ip 留空 + enable_addr_auto:由 mdns-sd 自动填充本机所有局域网地址并随网卡变化更新。
    let service = ServiceInfo::new(
        SERVICE_TYPE,
        host_label,
        host_name.as_str(),
        "",
        port,
        None::<std::collections::HashMap<String, String>>,
    )
    .map_err(|e| format!("mdns service info: {e}"))?
    .enable_addr_auto();
    daemon
        .register(service)
        .map_err(|e| format!("mdns register: {e}"))?;
    tracing::info!(host = %host_name, port, "mDNS advertising onchina console");
    // daemon 与本线程同生命周期:park 长驻,进程退出时随之回收。
    loop {
        std::thread::park();
    }
}

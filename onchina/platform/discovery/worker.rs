//! 既有 mDNS 服务注册实现。
use super::SERVICE_TYPE;
use mdns_sd::{ServiceDaemon, ServiceInfo};
pub(super) fn run_advertise(host_label: &str, port: u16) -> Result<(), String> {
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

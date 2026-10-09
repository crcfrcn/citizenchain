// 清算行节点声明前的对外可达性自测。
//
// 4 重校验(任一失败提交按钮置灰):
//   1. DNS 解析 — 域名能解出 IPv4/IPv6 地址
//   2. wss 链路 — 远端 RPC 端口能通,握手能完
//   3. 链 ID 匹配 — system_properties.ss58Format == 2027(GMB 主链)
//   4. PeerId 匹配 — system_localPeerId 与本机 PeerId 完全一致(防 DNS 劫持)
//
// 若域名/端口指向的是另一台节点,但 chain_id 仍然合法,会在 PeerId 匹配步骤失败,
// 同样阻止注册。校验目的是确保用户填的"对外服务地址"真的指向**本机节点**。

use serde_json::Value;
use std::net::ToSocketAddrs;
use std::time::Duration;

use crate::shared::{constants::RPC_RESPONSE_LIMIT_SMALL, rpc};

use crate::transaction::offchain::types::{ConnectivityCheck, ConnectivityTestReport};

const DNS_RESOLVE_TIMEOUT: Duration = Duration::from_secs(3);
const REMOTE_RPC_TIMEOUT: Duration = Duration::from_secs(5);
const EXPECTED_SS58_PREFIX: u64 = primitives::core_const::SS58_FORMAT as u64;

/// 对 `domain:port` 做 4 重连通性自测,返回前端友好的逐项报告。
pub fn run_endpoint_connectivity_test(
    domain: &str,
    port: u16,
    expected_peer_id: &str,
) -> ConnectivityTestReport {
    let mut checks: Vec<ConnectivityCheck> = Vec::with_capacity(4);

    // 1. DNS 解析(标准库 to_socket_addrs 自带 timeout 由 OS 决定,这里手动包一层)
    let dns_check = run_dns_check(domain);
    let dns_ok = dns_check.ok;
    checks.push(dns_check);

    // 2/3/4 都依赖远端 wss → 用 HTTP RPC 9944 等价探测一个 system_properties + system_localPeerId
    if !dns_ok {
        // DNS 不通时后续校验全部跳过,避免进一步暴露错误信息
        checks.push(skip_check("远端 RPC 链路", "DNS 未解析,跳过链路探测"));
        checks.push(skip_check("链 ID 匹配", "DNS 未解析,跳过链 ID 校验"));
        checks.push(skip_check("PeerId 匹配", "DNS 未解析,跳过 PeerId 校验"));
        return finalize(checks);
    }

    // 远端 HTTP RPC URL(节点桌面端这里探测的是 wss/RPC 是否上线;HTTP 9944 与 wss 共用 substrate
    // jsonrpsee server,达到任一即代表节点 Listen 正常。SS58 prefix 校验也借这条链路完成)。
    let url = format!("https://{domain}:{port}/");

    let props = match rpc::rpc_post_url(
        &url,
        "system_properties",
        Value::Array(vec![]),
        REMOTE_RPC_TIMEOUT,
        RPC_RESPONSE_LIMIT_SMALL,
    ) {
        Ok(v) => {
            checks.push(ConnectivityCheck {
                label: "远端 RPC 链路",
                ok: true,
                detail: None,
            });
            Some(v)
        }
        Err(e) => {
            checks.push(ConnectivityCheck {
                label: "远端 RPC 链路",
                ok: false,
                detail: Some(format!("无法连接 {url}:{e}")),
            });
            None
        }
    };

    let chain_ok = match props.as_ref() {
        Some(v) => {
            let ss58 = v
                .get("ss58Format")
                .and_then(|v| {
                    v.as_u64()
                        .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
                })
                .unwrap_or(0);
            if ss58 == EXPECTED_SS58_PREFIX {
                checks.push(ConnectivityCheck {
                    label: "链 ID 匹配",
                    ok: true,
                    detail: None,
                });
                true
            } else {
                checks.push(ConnectivityCheck {
                    label: "链 ID 匹配",
                    ok: false,
                    detail: Some(format!(
                        "ss58Format={ss58},不是 GMB 主链 prefix={EXPECTED_SS58_PREFIX}"
                    )),
                });
                false
            }
        }
        None => {
            checks.push(skip_check("链 ID 匹配", "远端 RPC 不通,跳过"));
            false
        }
    };

    // 4. PeerId 匹配 — 远端 system_localPeerId 必须与 expected_peer_id 完全一致
    if !chain_ok {
        checks.push(skip_check("PeerId 匹配", "链 ID 不匹配,跳过 PeerId 校验"));
        return finalize(checks);
    }

    match rpc::rpc_post_url(
        &url,
        "system_localPeerId",
        Value::Array(vec![]),
        REMOTE_RPC_TIMEOUT,
        RPC_RESPONSE_LIMIT_SMALL,
    ) {
        Ok(v) => {
            let remote_pid = v.as_str().unwrap_or("").trim().to_string();
            if remote_pid == expected_peer_id {
                checks.push(ConnectivityCheck {
                    label: "PeerId 匹配",
                    ok: true,
                    detail: None,
                });
            } else {
                checks.push(ConnectivityCheck {
                    label: "PeerId 匹配",
                    ok: false,
                    detail: Some(format!(
                        "远端 PeerId 不一致,期望 {expected_peer_id},实际 {remote_pid}"
                    )),
                });
            }
        }
        Err(e) => checks.push(ConnectivityCheck {
            label: "PeerId 匹配",
            ok: false,
            detail: Some(format!("读取远端 PeerId 失败:{e}")),
        }),
    }

    finalize(checks)
}

fn run_dns_check(domain: &str) -> ConnectivityCheck {
    if domain.is_empty() {
        return ConnectivityCheck {
            label: "DNS 解析",
            ok: false,
            detail: Some("域名为空".to_string()),
        };
    }

    let target = format!("{domain}:443");
    let (tx, rx) = std::sync::mpsc::channel::<Result<usize, String>>();
    std::thread::spawn(move || {
        let r = match (target.as_str()).to_socket_addrs() {
            Ok(addrs) => Ok(addrs.count()),
            Err(e) => Err(format!("DNS 解析失败:{e}")),
        };
        let _ = tx.send(r);
    });

    match rx.recv_timeout(DNS_RESOLVE_TIMEOUT) {
        Ok(Ok(count)) if count > 0 => ConnectivityCheck {
            label: "DNS 解析",
            ok: true,
            detail: None,
        },
        Ok(Ok(_)) => ConnectivityCheck {
            label: "DNS 解析",
            ok: false,
            detail: Some("DNS 解析返回空地址列表".to_string()),
        },
        Ok(Err(e)) => ConnectivityCheck {
            label: "DNS 解析",
            ok: false,
            detail: Some(e),
        },
        Err(_) => ConnectivityCheck {
            label: "DNS 解析",
            ok: false,
            detail: Some(format!(
                "DNS 解析超时(>{}秒)",
                DNS_RESOLVE_TIMEOUT.as_secs()
            )),
        },
    }
}

fn skip_check(label: &'static str, detail: &str) -> ConnectivityCheck {
    ConnectivityCheck {
        label,
        ok: false,
        detail: Some(detail.to_string()),
    }
}

fn finalize(checks: Vec<ConnectivityCheck>) -> ConnectivityTestReport {
    let all_ok = checks.iter().all(|c| c.ok);
    ConnectivityTestReport { all_ok, checks }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_domain_dns_check_fails_fast() {
        let c = run_dns_check("");
        assert!(!c.ok);
        assert_eq!(c.label, "DNS 解析");
    }

    #[test]
    fn finalize_marks_all_ok_when_every_check_passes() {
        let checks = vec![
            ConnectivityCheck {
                label: "a",
                ok: true,
                detail: None,
            },
            ConnectivityCheck {
                label: "b",
                ok: true,
                detail: None,
            },
        ];
        let r = finalize(checks);
        assert!(r.all_ok);
    }

    #[test]
    fn finalize_flags_failure_when_any_fails() {
        let checks = vec![
            ConnectivityCheck {
                label: "a",
                ok: true,
                detail: None,
            },
            ConnectivityCheck {
                label: "b",
                ok: false,
                detail: Some("x".into()),
            },
        ];
        let r = finalize(checks);
        assert!(!r.all_ok);
    }
}

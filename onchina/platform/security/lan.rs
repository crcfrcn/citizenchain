//! 只为宿主选择物理局域网监听地址。同机业务模块继续监听回环地址。
use std::net::{Ipv4Addr, SocketAddr};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Medium {
    Ethernet,
    Wifi,
}

struct Candidate {
    medium: Medium,
    priority: (usize, u32),
    ip: Ipv4Addr,
}

fn select(candidates: impl IntoIterator<Item = Candidate>) -> Option<Ipv4Addr> {
    candidates
        .into_iter()
        .filter(|candidate| candidate.ip.is_private())
        .min_by_key(|candidate| (candidate.medium, candidate.priority, candidate.ip))
        .map(|candidate| candidate.ip)
}

pub(crate) fn listen_address(port: u16) -> Result<SocketAddr, String> {
    select(candidates()?)
        .map(|ip| SocketAddr::from((ip, port)))
        .ok_or_else(|| "未检测到可用的有线或 Wi-Fi 局域网地址".to_string())
}

#[cfg(target_os = "macos")]
fn candidates() -> Result<Vec<Candidate>, String> {
    use system_configuration::{
        core_foundation::{
            array::CFArray,
            base::{CFType, TCFType},
            boolean::CFBoolean,
            dictionary::CFDictionary,
            string::CFString,
            ConcreteCFType,
        },
        dynamic_store::{SCDynamicStore, SCDynamicStoreBuilder},
        network_configuration::{get_interfaces, SCNetworkInterfaceType},
    };

    fn property<T: ConcreteCFType>(store: &SCDynamicStore, path: &str, key: &str) -> Option<T> {
        let value = store.get(path)?.downcast_into::<CFDictionary>()?;
        // SAFETY: SystemConfiguration 状态字典的键为 CFString，值为 Core Foundation 对象。
        #[allow(unsafe_code)]
        let dictionary = unsafe {
            CFDictionary::<CFString, CFType>::wrap_under_get_rule(value.as_concrete_TypeRef())
        };
        dictionary.find(CFString::new(key))?.downcast::<T>()
    }

    fn strings(store: &SCDynamicStore, path: &str, key: &str) -> Vec<String> {
        let Some(array) = property::<CFArray>(store, path, key) else {
            return Vec::new();
        };
        // SAFETY: 状态属性列表的数组元素为 Core Foundation 对象；逐个检查字符串类型。
        #[allow(unsafe_code)]
        let values = unsafe { CFArray::<CFType>::wrap_under_get_rule(array.as_concrete_TypeRef()) };
        values
            .iter()
            .filter_map(|value| value.downcast::<CFString>().map(|value| value.to_string()))
            .collect()
    }

    let store = SCDynamicStoreBuilder::new("OnChina LAN")
        .build()
        .ok_or_else(|| "无法读取 macOS 网络配置".to_string())?;
    let service_order = strings(&store, "Setup:/Network/Global/IPv4", "ServiceOrder")
        .iter()
        .map(|id| {
            property::<CFString>(
                &store,
                &format!("Setup:/Network/Service/{id}/Interface"),
                "DeviceName",
            )
            .map(|name| name.to_string())
        })
        .collect::<Vec<_>>();

    let mut result = Vec::new();
    for interface in get_interfaces().iter() {
        let medium = match interface.interface_type() {
            Some(SCNetworkInterfaceType::Ethernet) => Medium::Ethernet,
            Some(SCNetworkInterfaceType::IEEE80211) => Medium::Wifi,
            _ => continue,
        };
        let Some(name) = interface.bsd_name().map(|name| name.to_string()) else {
            continue;
        };
        let state = format!("State:/Network/Interface/{name}");
        if !property::<CFBoolean>(&store, &format!("{state}/Link"), "Active")
            .is_some_and(bool::from)
        {
            continue;
        }
        let priority = service_order
            .iter()
            .position(|device| device.as_deref() == Some(name.as_str()))
            .unwrap_or(usize::MAX);
        let addresses = strings(&store, &format!("{state}/IPv4"), "Addresses");
        result.extend(
            addresses
                .iter()
                .filter_map(|address| address.parse().ok())
                .map(|ip| Candidate {
                    medium,
                    priority: (priority, 0),
                    ip,
                }),
        );
    }
    Ok(result)
}

#[cfg(any(target_os = "linux", windows))]
fn active_ipv4s() -> Result<Vec<(if_addrs::Interface, Ipv4Addr)>, String> {
    use std::net::IpAddr;

    let interfaces =
        if_addrs::get_if_addrs().map_err(|error| format!("无法读取本机网络接口：{error}"))?;
    Ok(interfaces
        .into_iter()
        .filter_map(|interface| {
            let IpAddr::V4(ip) = interface.ip() else {
                return None;
            };
            (interface.is_oper_up() && !interface.is_p2p() && ip.is_private())
                .then_some((interface, ip))
        })
        .collect())
}

#[cfg(target_os = "linux")]
fn candidates() -> Result<Vec<Candidate>, String> {
    use std::{collections::HashMap, fs, path::Path};

    // 同类网卡以系统 IPv4 默认路由的 metric 排序，无默认路由时以接口索引排序。
    let mut metrics = HashMap::new();
    let routes = fs::read_to_string("/proc/net/route").unwrap_or_default();
    for line in routes.lines().skip(1) {
        let fields: Vec<_> = line.split_whitespace().collect();
        if let [name, "00000000", _, flags, _, _, metric, "00000000", ..] = fields.as_slice() {
            if u32::from_str_radix(flags, 16).is_ok_and(|value| value & 1 != 0) {
                if let Ok(metric) = metric.parse::<usize>() {
                    metrics
                        .entry(*name)
                        .and_modify(|old: &mut usize| *old = (*old).min(metric))
                        .or_insert(metric);
                }
            }
        }
    }
    Ok(active_ipv4s()?
        .into_iter()
        .filter_map(|(interface, ip)| {
            let device = Path::new("/sys/class/net").join(&interface.name);
            // 排除 veth、网桥、VPN 等虚拟设备，且要求物理链路已连接。
            if !device.join("device").exists()
                || !fs::read_to_string(device.join("type")).is_ok_and(|value| value.trim() == "1")
                || !fs::read_to_string(device.join("carrier"))
                    .is_ok_and(|value| value.trim() == "1")
            {
                return None;
            }
            let medium = if device.join("wireless").exists() || device.join("phy80211").exists() {
                Medium::Wifi
            } else {
                Medium::Ethernet
            };
            let priority = (
                metrics
                    .get(interface.name.as_str())
                    .copied()
                    .unwrap_or(usize::MAX),
                interface.index.unwrap_or(u32::MAX),
            );
            Some(Candidate {
                medium,
                priority,
                ip,
            })
        })
        .collect())
}

#[cfg(windows)]
fn candidates() -> Result<Vec<Candidate>, String> {
    use windows_sys::Win32::{
        NetworkManagement::{
            IpHelper::{
                GetIfEntry2, GetIpInterfaceEntry, IF_TYPE_ETHERNET_CSMACD, IF_TYPE_IEEE80211,
                MIB_IF_ROW2, MIB_IPINTERFACE_ROW,
            },
            Ndis::MediaConnectStateConnected,
        },
        Networking::WinSock::AF_INET,
    };

    Ok(active_ipv4s()?
        .into_iter()
        .filter_map(|(interface, ip)| {
            let index = interface.index?;
            let mut row = MIB_IF_ROW2 {
                InterfaceIndex: index,
                ..Default::default()
            };
            // SAFETY: 接口索引由系统枚举提供，row 为正确类型的可写缓冲区。
            #[allow(unsafe_code)]
            let result = unsafe { GetIfEntry2(&mut row) };
            // MIB_IF_ROW2 的第 0 位是 HardwareInterface，用于排除虚拟以太网。
            if result != 0
                || row.InterfaceAndOperStatusFlags._bitfield & 1 == 0
                || row.MediaConnectState != MediaConnectStateConnected
            {
                return None;
            }
            let medium = match row.Type {
                IF_TYPE_ETHERNET_CSMACD => Medium::Ethernet,
                IF_TYPE_IEEE80211 => Medium::Wifi,
                _ => return None,
            };
            let mut config = MIB_IPINTERFACE_ROW {
                Family: AF_INET,
                InterfaceIndex: index,
                ..Default::default()
            };
            // SAFETY: 用 AF_INET 和同一接口索引查询；config 为正确类型的可写缓冲区。
            #[allow(unsafe_code)]
            let result = unsafe { GetIpInterfaceEntry(&mut config) };
            let metric = if result == 0 {
                config.Metric as usize
            } else {
                usize::MAX
            };
            Some(Candidate {
                medium,
                priority: (metric, index),
                ip,
            })
        })
        .collect())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn candidates() -> Result<Vec<Candidate>, String> {
    Err("当前系统不支持自动识别链上中国的局域网地址".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(medium: Medium, rank: usize, ip: [u8; 4]) -> Candidate {
        Candidate {
            medium,
            priority: (rank, 0),
            ip: ip.into(),
        }
    }

    #[test]
    fn wired_priority_precedes_wifi_and_uses_system_order() {
        assert_eq!(
            select([
                candidate(Medium::Wifi, 0, [172, 16, 0, 3]),
                candidate(Medium::Ethernet, 4, [192, 168, 0, 4]),
                candidate(Medium::Ethernet, 2, [10, 0, 0, 2]),
            ]),
            Some(Ipv4Addr::new(10, 0, 0, 2))
        );
    }

    #[test]
    fn only_usable_lan_addresses_are_selected() {
        assert_eq!(
            select([
                candidate(Medium::Ethernet, 0, [169, 254, 0, 2]),
                candidate(Medium::Wifi, 1, [172, 16, 0, 3]),
            ]),
            Some(Ipv4Addr::new(172, 16, 0, 3))
        );
        assert_eq!(
            select([
                candidate(Medium::Ethernet, 0, [8, 8, 8, 8]),
                candidate(Medium::Wifi, 0, [127, 0, 0, 1]),
            ]),
            None
        );
        // 接口发现已排除 VPN；只有 VPN 或断网时不会产生候选。
        assert_eq!(select([]), None);
    }
}

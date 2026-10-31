//! 原所属模块回归，断言与输入保持。
use super::database_url_targets_local_host_only;

// 只解析配置，不连接数据库；各原生宿主共用相同TCP判断边界。
#[test]
fn tcp_database_hosts_require_every_host_to_be_local() {
    for value in [
        "user=citizenchain dbname=onchina",
        "host=localhost user=citizenchain",
        "host=LOCALHOST user=citizenchain",
        "host=127.0.0.1 user=citizenchain",
        "host=::1 user=citizenchain",
        "host=localhost,127.0.0.1 user=citizenchain",
    ] {
        assert!(
            database_url_targets_local_host_only(value).unwrap(),
            "{value}"
        );
    }
    for value in [
        "host=db.example.invalid user=citizenchain",
        "host=0.0.0.0 user=citizenchain",
        "host=localhost,db.example.invalid user=citizenchain",
    ] {
        assert!(
            !database_url_targets_local_host_only(value).unwrap(),
            "{value}"
        );
    }
    assert!(database_url_targets_local_host_only("host").is_err());
}

#[cfg(unix)]
#[test]
fn unix_database_sockets_remain_local_without_accepting_remote_peers() {
    assert!(database_url_targets_local_host_only("host=/tmp user=citizenchain").unwrap());
    assert!(!database_url_targets_local_host_only(
        "host=/tmp,db.example.invalid user=citizenchain"
    )
    .unwrap());
}

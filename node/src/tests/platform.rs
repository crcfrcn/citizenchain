//! 验证模块路径交接、宿主身份、公开快捷入口及子进程归属。

use super::*;

use crate::platform::wait_onchina_exit;
use std::process::Child;

// 使用当前测试程序作为子进程，覆盖Windows/Unix父管道；不启动业务、数据库或链。
#[test]
#[ignore = "仅供父进程测试通过精确名称拉起"]
fn pipe_child() {
    if std::env::var_os("ONCHINA_PIPE_CHILD_TEST").is_none() {
        return;
    }
    use std::io::Read;
    // 独立子进程持有真实端口，验证停止本次宿主不会终止其他实例或释放其端口。
    let listener = std::env::var_os("ONCHINA_PIPE_CHILD_LISTEN").map(|_| {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .expect("bind other child port");
        println!("{}", listener.local_addr().expect("other child address"));
        std::io::stdout().flush().expect("publish child address");
        listener
    });
    let mut bytes = Vec::new();
    std::io::stdin()
        .read_to_end(&mut bytes)
        .expect("read parent pipe");
    drop(listener);
    if std::env::var_os("ONCHINA_PIPE_CHILD_FAIL").is_some() {
        std::process::exit(7);
    }
}

fn spawn_pipe_child(fail: bool) -> Child {
    let mut command = Command::new(std::env::current_exe().expect("test executable"));
    command
        .args([
            "--ignored",
            "--exact",
            "platform::lifecycle_tests::pipe_child",
        ])
        .env("ONCHINA_PIPE_CHILD_TEST", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if fail {
        command.env("ONCHINA_PIPE_CHILD_FAIL", "1");
    }
    command.spawn().expect("spawn child")
}

#[test]
fn parent_pipe_closes_before_success_and_timeout_keeps_handle() {
    let mut child = spawn_pipe_child(false);
    assert!(wait_onchina_exit(&mut child, Duration::from_millis(20)).is_err());
    assert!(child.try_wait().expect("child status").is_none());
    drop(child.stdin.take());
    wait_onchina_exit(&mut child, Duration::from_secs(5)).expect("reap after EOF");
    wait_onchina_exit(&mut child, Duration::ZERO).expect("repeat reap");
}

#[test]
fn abnormal_exit_is_not_reported_as_success() {
    let mut child = spawn_pipe_child(true);
    drop(child.stdin.take());
    assert!(wait_onchina_exit(&mut child, Duration::from_secs(5)).is_err());
    assert!(!child.wait().expect("reap failed child").success());
}

#[test]
fn stopping_owned_child_preserves_other_process_and_occupied_port() {
    use std::io::BufRead;
    let mut other = Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--ignored",
            "--exact",
            "platform::lifecycle_tests::pipe_child",
            "--nocapture",
        ])
        .env("ONCHINA_PIPE_CHILD_TEST", "1")
        .env("ONCHINA_PIPE_CHILD_LISTEN", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn other child");
    let mut output = std::io::BufReader::new(other.stdout.take().expect("other child stdout"));
    let address = loop {
        let mut line = String::new();
        assert_ne!(output.read_line(&mut line).expect("read child address"), 0);
        if let Ok(address) = line.trim().parse::<SocketAddr>() {
            break address;
        }
    };
    assert!(std::net::TcpListener::bind(address).is_err());
    let mut owned = spawn_pipe_child(false);
    drop(owned.stdin.take());
    wait_onchina_exit(&mut owned, Duration::from_secs(5)).expect("stop owned child");
    assert!(other.try_wait().expect("other process status").is_none());
    assert!(std::net::TcpListener::bind(address).is_err());
    drop(other.stdin.take());
    wait_onchina_exit(&mut other, Duration::from_secs(5)).expect("reap other child");
    assert!(std::net::TcpListener::bind(address).is_ok());
}

#[test]
fn module_paths_separate_bundle_from_activity_without_creating_directories() {
    let base = std::env::temp_dir().join(format!("onchina-paths-{}", std::process::id()));
    let resources = base.join("application resources");
    assert!(!base.exists());
    let (modules, bundle) = module_paths(
        &base,
        Some(&resources),
        Some("ignored".into()),
        Some("ignored".into()),
    )
    .unwrap();
    assert_eq!(modules, base.join("modules"));
    assert_eq!(bundle, Some(resources.join("onchina-bin/modules")));
    assert!(!base.exists());

    // 无随包PG/资源时仍交付活动根；正式与开发命名空间不共用模块目录。
    let development = base.join("gmb.dev");
    assert_eq!(
        module_paths(&development, None, None, None).unwrap(),
        (development.join("modules"), None)
    );
    assert_eq!(
        module_paths(&base.join("gmb"), None, None, None).unwrap(),
        (base.join("gmb/modules"), None)
    );
    let explicit = base.join("custom modules");
    let source = base.join("initial packages");
    assert_eq!(
        module_paths(
            &development,
            None,
            Some(explicit.clone().into_os_string()),
            Some(source.clone().into_os_string())
        )
        .unwrap(),
        (explicit, Some(source))
    );
    assert!(!base.exists());
}

#[test]
fn module_paths_reject_invalid_inputs_without_defaulting() {
    let base = std::env::temp_dir();
    for invalid in ["", "relative/modules"] {
        assert_eq!(
            module_paths(&base, None, Some(invalid.into()), None).unwrap_err(),
            "ONCHINA_MODULE_DIR 必须是非空绝对路径"
        );
        assert_eq!(
            module_paths(&base, None, None, Some(invalid.into())).unwrap_err(),
            "ONCHINA_BUNDLE_DIR 必须是非空绝对路径"
        );
    }
    assert!(module_paths(Path::new("relative"), None, None, None).is_err());
    assert!(module_paths(&base, Some(Path::new("relative")), None, None).is_err());
}

#[test]
fn existing_data_paths_are_preserved() {
    let base = std::env::temp_dir().join("onchina data");
    let mut command = Command::new("onchina");
    apply_data_dir_env(&base, &mut command);
    let values = command
        .get_envs()
        .map(|(key, value)| (key.to_os_string(), value.unwrap().to_os_string()))
        .collect::<std::collections::BTreeMap<_, _>>();
    for (key, directory) in [
        ("ONCHINA_PG_DATA_DIR", "pgdata"),
        ("ONCHINA_TLS_DIR", "onchina-tls"),
        ("ONCHINA_PG_WAL_ARCHIVE_DIR", "pg-wal-archive"),
    ] {
        assert_eq!(
            values.get(std::ffi::OsStr::new(key)),
            Some(&base.join(directory).into_os_string())
        );
    }
    assert_eq!(values.len(), 3);
}

#[test]
fn health_only_accepts_host_and_exposes_existing_public_shortcuts() {
    let host = serde_json::json!({"code":0,"data":{
        "service":"onchina","status":"UP","module_id":null,
        "modules":[{"id":"registry"},{"id":"private_package"},{"id":"legislation"},{"id":"judicial"},{"id":"private"}],
        "interface_version":999,"module_version":"99.0.0","future_capability":true
    }});
    assert_eq!(
        parse_health_modules(&serde_json::to_vec(&host).unwrap()).unwrap(),
        ["registry", "legislation", "judicial", "private"]
    );
    let mut empty = host.clone();
    empty["data"]["modules"] = serde_json::json!([]);
    assert!(parse_health_modules(&serde_json::to_vec(&empty).unwrap())
        .unwrap()
        .is_empty());
    let address = "192.168.1.4:1988".parse().unwrap();
    assert_eq!(
        platform_state(true, Some(address), |_| parse_health_modules(
            &serde_json::to_vec(&empty).unwrap()
        ))
        .status,
        "enabled"
    );

    for (pointer, value) in [
        ("/code", serde_json::json!(1)),
        ("/data/service", serde_json::json!("another")),
        ("/data/status", serde_json::json!("DEGRADED")),
        ("/data/module_id", serde_json::json!("registry")),
    ] {
        let mut invalid = host.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        assert_eq!(
            parse_health_modules(&serde_json::to_vec(&invalid).unwrap()).unwrap_err(),
            "链上中国平台健康检查未通过"
        );
    }
    for bytes in [
        b"not json".as_slice(),
        b"{}",
        br#"{"code":0,"data":{"status":"UP","modules":[]}}"#,
    ] {
        assert_eq!(
            parse_health_modules(bytes).unwrap_err(),
            "链上中国健康检查响应无效"
        );
    }
}

#[test]
fn deployment_is_read_from_product_description_and_rejects_ambiguous_endpoints() {
    let receipt = serde_json::json!({"id":"platform","deployment":{
        "host":"onchina.crcfrcn.com","port":1988,"url":"https://onchina.crcfrcn.com:1988/"
    }});
    let value = parse_deployment(&serde_json::to_vec(&receipt).unwrap()).unwrap();
    assert_eq!(value.url, "https://onchina.crcfrcn.com:1988/");
    assert_eq!(value.port, 1988);
    for url in [
        "http://onchina.crcfrcn.com:1988/",
        "https://wrong.invalid:1988/",
        "https://onchina.crcfrcn.com:1989/",
        "https://onchina.crcfrcn.com:1988/path",
        "https://user:secret@onchina.crcfrcn.com:1988/",
        "https://onchina.crcfrcn.com:1988/?query=1",
    ] {
        let mut invalid = receipt.clone();
        invalid["deployment"]["url"] = serde_json::Value::String(url.into());
        assert!(parse_deployment(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
    for port in [0, 1989] {
        let mut invalid = receipt.clone();
        invalid["deployment"]["port"] = serde_json::Value::from(port);
        assert!(parse_deployment(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
    assert!(parse_deployment(br#"{"id":"platform"}"#).is_err());
}

#[test]
fn listen_address_is_validated_and_scoped_to_the_current_host() {
    for invalid in [
        "127.0.0.1:1988",
        "0.0.0.0:1988",
        "8.8.8.8:1988",
        "192.168.1.4:1989",
    ] {
        assert!(parse_listen_address(invalid, 1988).is_none());
    }
    let first = parse_listen_address("192.168.1.4:1988", 1988).unwrap();
    let second = parse_listen_address("172.16.1.5:1988", 1988).unwrap();

    begin_local_platform(1);
    record_local_platform_address(1, first);
    assert_eq!(local_platform_address(), Some(first));
    end_local_platform(1);
    assert_eq!(local_platform_address(), None);

    begin_local_platform(2);
    record_local_platform_address(1, first);
    assert_eq!(local_platform_address(), None);
    record_local_platform_address(2, second);
    end_local_platform(1);
    assert_eq!(local_platform_address(), Some(second));
    end_local_platform(2);
    assert_eq!(local_platform_address(), None);
}

#[test]
fn startup_waits_for_https_address_and_preserves_health_failures() {
    let address = parse_listen_address("192.168.1.4:1988", 1988).unwrap();
    let waiting = platform_state(true, None, |_| panic!("HTTPS 未就绪时不应发起请求"));
    assert!(waiting.running);
    assert_eq!(waiting.status, "starting");
    assert_eq!(waiting.status_label, "启动中");
    assert_eq!(waiting.detail, None);
    assert!(waiting.modules.is_empty());

    let ready = platform_state(true, Some(address), |target| {
        assert_eq!(target, address);
        Ok(vec!["registry".into()])
    });
    assert_eq!(ready.status, "enabled");
    assert_eq!(ready.detail, None);
    assert_eq!(ready.modules, vec!["registry"]);

    for error in [
        "链上中国可信 HTTPS 健康检查失败",
        "链上中国健康检查响应无效",
        "链上中国平台健康检查未通过",
    ] {
        let failed = platform_state(true, Some(address), |_| Err(error.into()));
        assert_eq!(failed.status, "starting");
        assert_eq!(failed.detail.as_deref(), Some(error));
        assert!(failed.modules.is_empty());
    }

    let stopped = platform_state(false, Some(address), |_| panic!("已停止时不应发起请求"));
    assert!(!stopped.running);
    assert_eq!(stopped.status, "stopped");
    assert_eq!(stopped.detail, None);
    let exited = failed_state(false, "链上中国子进程已退出".into());
    assert_eq!(exited.status, "error");
    assert_eq!(exited.detail.as_deref(), Some("链上中国子进程已退出"));
}

#[test]
fn module_link_only_uses_the_deployment_origin_and_loaded_modules() {
    let base = reqwest::Url::parse("https://product.example:1988/").unwrap();
    let loaded = vec!["registry".to_string(), "https://other.example/".to_string()];
    let result = onchina_module_entry(base.clone(), "registry", &loaded).unwrap();
    assert_eq!(
        result.as_str(),
        "https://product.example:1988/?module=registry"
    );
    assert!(onchina_module_entry(base.clone(), "judicial", &loaded).is_err());
    assert!(onchina_module_entry(base, "https://other.example/", &loaded).is_err());
}

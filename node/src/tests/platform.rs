//! 验证链上中国子进程管道、就绪、超时及异常退出。

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
    let mut bytes = Vec::new();
    std::io::stdin()
        .read_to_end(&mut bytes)
        .expect("read parent pipe");
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

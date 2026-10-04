//! 设置页“链上中国平台”手动启动入口。
//!
//! 节点软件默认只启动区块链节点;链上中国平台服务占用数据库、HTTPS 和
//! 浏览器管理后台资源,因此必须由用户在设置页二次确认后显式启动。

use serde::Serialize;
use std::time::Duration;
use tauri::AppHandle;

use crate::shared::security;

/// 节点安装后管理员在局域网浏览器中访问的唯一固定入口。
const ONCHINA_PLATFORM_URL: &str = "https://onchina.local:8964";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnChinaPlatformState {
    running: bool,
    status: &'static str,
    status_label: &'static str,
    url: &'static str,
    detail: Option<String>,
}

// 中文注释：只信任本机构 CA，并固定 onchina.local 的连接到本机；不跳过证书/主机名校验。
fn onchina_health_ok(app: &AppHandle) -> Result<(), String> {
    let path = security::app_data_dir(app)?
        .join("onchina-tls")
        .join("onchina-org-root-ca.crt");
    let metadata =
        std::fs::symlink_metadata(&path).map_err(|_| "链上中国机构 CA 未就绪".to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("链上中国机构 CA 文件无效".to_string());
    }
    let bytes = std::fs::read(path).map_err(|_| "读取链上中国机构 CA 失败".to_string())?;
    let ca = reqwest::Certificate::from_pem(&bytes)
        .map_err(|_| "链上中国机构 CA 格式无效".to_string())?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(900))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .tls_certs_only([ca])
        .resolve(
            "onchina.local",
            std::net::SocketAddr::from(([127, 0, 0, 1], 8964)),
        )
        .build()
        .map_err(|e| format!("创建链上中国健康检查客户端失败:{e}"))?;
    let response = client
        .get("https://onchina.local:8964/api/health")
        .send()
        .map_err(|_| "链上中国可信 HTTPS 健康检查失败".to_string())?;
    if !response.status().is_success() {
        return Err("链上中国健康检查状态无效".to_string());
    }
    let payload = response
        .json::<serde_json::Value>()
        .map_err(|_| "链上中国健康检查响应无效".to_string())?;
    if payload
        .get("data")
        .and_then(|data| data.get("status"))
        .and_then(|value| value.as_str())
        == Some("UP")
    {
        Ok(())
    } else {
        Err("链上中国平台健康检查未通过".to_string())
    }
}

fn current_state(app: &AppHandle) -> OnChinaPlatformState {
    let process_running = crate::onchina_proc::is_onchina_running();
    let health = process_running.then(|| onchina_health_ok(app));
    let (status, status_label, detail) = match health {
        None => ("stopped", "未开启", None),
        Some(Ok(())) => ("enabled", "已开启", None),
        // 进程已启动但健康检查暂未通过时，只能表达“启动中”。
        // 失败原因必须等启动动作最终超时或显式失败后再返回 error，避免界面同时显示
        // “启动中”和“启动失败”两套互相冲突的状态。
        Some(Err(_)) => ("starting", "启动中", None),
    };
    OnChinaPlatformState {
        running: process_running,
        status,
        status_label,
        url: ONCHINA_PLATFORM_URL,
        detail,
    }
}

fn failed_state(running: bool, detail: String) -> OnChinaPlatformState {
    OnChinaPlatformState {
        running,
        status: "error",
        status_label: "启动失败",
        url: ONCHINA_PLATFORM_URL,
        detail: Some(detail),
    }
}

fn wait_until_healthy_after_start(app: &AppHandle) -> OnChinaPlatformState {
    let mut last_error = "链上中国平台健康检查未通过".to_string();
    // 上限 120 次 × 500ms = 60 秒;健康一通过即提前返回。首次内嵌 PostgreSQL 要
    // initdb 建集群 + 建库 + 迁移,明显超过原来的 10 秒预算,过短会把还在初始化的
    // 正常启动误判成"启动失败"。子进程退出才是真失败,超时只表示还没就绪。
    for _ in 0..120 {
        let process_running = crate::onchina_proc::is_onchina_running();
        if !process_running {
            return failed_state(false, "链上中国平台启动失败:子进程已退出".to_string());
        }
        match onchina_health_ok(app) {
            Ok(()) => return current_state(app),
            Err(err) => last_error = err,
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    failed_state(
        crate::onchina_proc::is_onchina_running(),
        format!("链上中国平台启动失败:{last_error}"),
    )
}

#[tauri::command]
pub fn get_onchina_platform(app: AppHandle) -> Result<OnChinaPlatformState, String> {
    Ok(current_state(&app))
}

#[tauri::command]
pub fn start_onchina_platform(app: AppHandle) -> Result<OnChinaPlatformState, String> {
    if let Err(err) = security::append_audit_log(&app, "start_onchina_platform", "attempt") {
        eprintln!("[审计] start_onchina_platform attempt 日志写入失败:{err}");
    }
    if let Err(err) = crate::onchina_proc::start_onchina(&app) {
        if let Err(log_err) = security::append_audit_log(&app, "start_onchina_platform", "failed") {
            eprintln!("[审计] start_onchina_platform failed 日志写入失败:{log_err}");
        }
        return Ok(failed_state(false, err));
    }
    if let Err(err) = security::append_audit_log(&app, "start_onchina_platform", "success") {
        eprintln!("[审计] start_onchina_platform success 日志写入失败:{err}");
    }
    Ok(wait_until_healthy_after_start(&app))
}

#[tauri::command]
pub fn stop_onchina_platform(app: AppHandle) -> Result<OnChinaPlatformState, String> {
    if let Err(err) = security::append_audit_log(&app, "stop_onchina_platform", "attempt") {
        eprintln!("[审计] stop_onchina_platform attempt 日志写入失败:{err}");
    }
    crate::onchina_proc::stop_onchina_checked()?;
    if let Err(err) = security::append_audit_log(&app, "stop_onchina_platform", "success") {
        eprintln!("[审计] stop_onchina_platform success 日志写入失败:{err}");
    }
    Ok(current_state(&app))
}

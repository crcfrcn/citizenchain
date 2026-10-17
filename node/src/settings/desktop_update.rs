//! 桌面端更新前置准备。
//!
//! 本模块按当前平台发现本仓成功Release；更新安装由Tauri updater负责，并在安装前停掉节点，
//! 确保 RocksDB 文件锁和后台线程先释放。

use crate::home;
use tauri::AppHandle;

#[tauri::command]
pub async fn prepare_desktop_update(app: AppHandle) -> Result<(), String> {
    // downloadAndInstall 会触发安装器和进程重启，安装前先停节点，避免节点数据目录仍被占用。
    tauri::async_runtime::spawn_blocking(move || home::stop_node_blocking(app).map(|_| ()))
        .await
        .map_err(|err| format!("prepare desktop update task failed: {err}"))?
}

// 当前平台只读取自己的成功Workflow，不使用跨平台latest或安装时的旧Tag。
fn update_target() -> Result<(&'static str, &'static str), String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok(("macos", "macOS")),
        ("windows", "x86_64") => Ok(("windows", "Windows")),
        ("linux", "aarch64") => Ok(("linux-arm", "LinuxARM")),
        ("linux", "x86_64") => Ok(("linux-amd", "LinuxAMD")),
        _ => Err("当前平台没有桌面更新目标".into()),
    }
}

fn workflow_file(platform: &str) -> Result<&'static str, String> {
    match platform {
        "macos" => Ok("macos.yml"),
        "windows" => Ok("windows.yml"),
        "linux-arm" => Ok("linuxarm.yml"),
        "linux-amd" => Ok("linuxamd.yml"),
        _ => Err("当前平台没有桌面更新自动化".into()),
    }
}

// 展示名与节点交付清单一致；版本来自已绑定成功Run的Release证明。
fn update_manifest_name(platform: &str, version: &str) -> Result<String, String> {
    if !matches!(platform, "macOS" | "Windows" | "LinuxARM" | "LinuxAMD") {
        return Err("更新清单平台无效".into());
    }
    let parts: Vec<_> = version.split('.').collect();
    if parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || part.len() > 1 && part.starts_with('0')
        })
    {
        return Err("更新清单版本无效".into());
    }
    Ok(format!("公民链·{platform}-v{version}.json"))
}

fn update_manifest_url(tag: &str, filename: &str) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(
        "https://github.com/crcfrcn/citizenchain/releases/download/",
    )
    .map_err(|_| "更新地址无效")?;
    url.path_segments_mut()
        .map_err(|_| "更新地址路径无效")?
        .push(tag)
        .push(filename);
    Ok(url)
}

fn release_proof(
    release: &serde_json::Value,
    run: &serde_json::Value,
    platform: &str,
) -> Option<serde_json::Value> {
    let workflow = format!(".github/workflows/{}", workflow_file(platform).ok()?);
    if release["draft"] != false || release["prerelease"] != false {
        return None;
    }
    let body = release["body"].as_str()?;
    let values: Vec<_> = body
        .lines()
        .filter_map(|line| line.strip_prefix("<!-- automation:")?.strip_suffix(" -->"))
        .collect();
    if values.len() != 1 {
        return None;
    }
    let proof: serde_json::Value = serde_json::from_str(values[0]).ok()?;
    if proof["schema"] != 1
        || proof["repository"] != "crcfrcn/citizenchain"
        || proof["product_id"] != "citizenchain"
        || proof["platform"] != platform
        || proof["workflow"] != workflow
        || proof["run_id"] != run["id"]
        || proof["run_attempt"] != run["run_attempt"]
        || proof["source_sha"] != run["head_sha"]
        || proof["tag"] != release["tag_name"]
    {
        return None;
    }
    Some(proof)
}

async fn github_json(client: &reqwest::Client, path: &str) -> Result<serde_json::Value, String> {
    let response = client
        .get(format!(
            "https://api.github.com/repos/crcfrcn/citizenchain/{path}"
        ))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| "更新元数据连接失败")?
        .error_for_status()
        .map_err(|_| "更新元数据读取失败")?;
    let mut response = response;
    let mut bytes = Vec::new();
    while let Some(part) = response.chunk().await.map_err(|_| "更新元数据传输失败")? {
        if bytes.len() + part.len() > 8 * 1024 * 1024 {
            return Err("更新元数据过大".into());
        }
        bytes.extend_from_slice(&part);
    }
    serde_json::from_slice(&bytes).map_err(|_| "更新元数据格式无效".into())
}

/// 从当前平台唯一成功Release检查更新，不复用安装时的旧Tag。
#[tauri::command]
pub async fn check_desktop_update(
    webview: tauri::Webview,
) -> Result<Option<serde_json::Value>, String> {
    use tauri::Manager;
    use tauri_plugin_updater::UpdaterExt;
    let (platform, manifest_platform) = update_target()?;
    let workflow = workflow_file(platform)?;
    let client = reqwest::Client::builder()
        .user_agent("CitizenChain")
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "更新客户端初始化失败")?;
    let mut runs = Vec::new();
    for page in 1.. {
        let value = github_json(&client, &format!("actions/workflows/{workflow}/runs?status=success&branch=main&event=workflow_dispatch&per_page=100&page={page}")).await?;
        let rows = value["workflow_runs"]
            .as_array()
            .ok_or("更新任务列表无效")?;
        runs.extend(
            rows.iter()
                .filter(|r| r["status"] == "completed" && r["conclusion"] == "success")
                .cloned(),
        );
        if rows.len() < 100 {
            break;
        }
    }
    if runs.is_empty() {
        return Ok(None);
    }
    let run = runs
        .iter()
        .max_by_key(|value| value["id"].as_u64().unwrap_or(0))
        .ok_or("当前平台成功运行记录无效")?;
    if run["path"] != format!(".github/workflows/{workflow}")
        || run["head_branch"] != "main"
        || run["event"] != "workflow_dispatch"
        || run["repository"]["full_name"] != "crcfrcn/citizenchain"
    {
        return Err("更新任务身份不符".into());
    }
    let mut matches = Vec::new();
    for page in 1.. {
        let value = github_json(&client, &format!("releases?per_page=100&page={page}")).await?;
        let rows = value.as_array().ok_or("更新Release列表无效")?;
        matches.extend(
            rows.iter()
                .filter_map(|r| release_proof(r, run, platform).map(|p| (r.clone(), p))),
        );
        if rows.len() < 100 {
            break;
        }
    }
    if matches.len() != 1 {
        return Err("当前平台成功Release不唯一".into());
    }
    let (release, proof) = &matches[0];
    let tag = proof["tag"].as_str().ok_or("更新Tag无效")?;
    if !tag.starts_with(&format!("citizenchain-{platform}-v"))
        || !tag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
    {
        return Err("更新Tag无效".into());
    }
    let reference = github_json(&client, &format!("git/ref/tags/{tag}")).await?;
    if reference["object"]["type"] != "commit" || reference["object"]["sha"] != run["head_sha"] {
        return Err("更新Tag提交不符".into());
    }
    let version = proof["version"].as_str().ok_or("更新证明版本缺失")?;
    let filename = update_manifest_name(manifest_platform, version)?;
    let assets = release["assets"].as_array().ok_or("更新资产列表无效")?;
    let proofs = proof["assets"].as_array().ok_or("更新资产证明无效")?;
    if assets.len() != proofs.len() {
        return Err("更新资产集合不完整".into());
    }
    for item in proofs {
        let matching: Vec<_> = assets
            .iter()
            .filter(|asset| asset["name"] == item["name"])
            .collect();
        let digest = format!(
            "sha256:{}",
            item["sha256"].as_str().ok_or("更新资产摘要无效")?
        );
        if matching.len() != 1
            || matching[0]["state"] != "uploaded"
            || matching[0]["size"] != item["size"]
            || matching[0]["digest"] != digest
        {
            return Err("更新资产证明不符".into());
        }
    }
    if assets
        .iter()
        .filter(|asset| asset["name"] == filename)
        .count()
        != 1
    {
        return Err("更新清单缺失或重复".into());
    }
    let endpoint = update_manifest_url(tag, &filename)?;
    let update = webview
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    if let Some(update) = update {
        if proof["version"] != update.version {
            return Err("更新清单版本不符".into());
        }
        let current_version = update.current_version.clone();
        let version = update.version.clone();
        let body = update.body.clone();
        let raw_json = update.raw_json.clone();
        let rid = webview.resources_table().add(update);
        Ok(Some(
            serde_json::json!({"rid":rid,"currentVersion":current_version,"version":version,"body":body,"rawJson":raw_json}),
        ))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod update_tests {
    use super::*;
    #[test]
    fn delivery_manifest_name_and_url_bind_platform_version_and_encoded_path() {
        for platform in ["macOS", "Windows", "LinuxARM", "LinuxAMD"] {
            let name = update_manifest_name(platform, "1.0.0").unwrap();
            assert_eq!(name, format!("公民链·{platform}-v1.0.0.json"));
            let url = update_manifest_url("citizenchain-macos-v1.0.0-r1-a1", &name).unwrap();
            assert_eq!(url.host_str(), Some("github.com"));
            assert_eq!(url.scheme(), "https");
            assert!(url.as_str().contains("%E5%85%AC%E6%B0%91%E9%93%BE%C2%B7"));
            assert!(url.as_str().ends_with(&format!("{platform}-v1.0.0.json")));
            assert_eq!(url.query(), None);
            assert_eq!(url.fragment(), None);
        }
        for version in ["", "1.0", "01.0.0", "1.0.0/other", "1.0.0?other", "1.0.0-rc1"] {
            assert!(update_manifest_name("macOS", version).is_err());
        }
        assert!(update_manifest_name("wasm", "1.0.0").is_err());
    }
    #[test]
    fn release_selection_is_platform_run_and_attempt_specific() {
        let run = serde_json::json!({"id":7,"run_attempt":2,"head_sha":"source"});
        let proof = serde_json::json!({"schema":1,"repository":"crcfrcn/citizenchain","product_id":"citizenchain","platform":"macos","workflow":".github/workflows/macos.yml","run_id":7,"run_attempt":2,"source_sha":"source","tag":"tag"});
        let mut release = serde_json::json!({"draft":false,"prerelease":false,"tag_name":"tag","body":format!("<!-- automation:{proof} -->")});
        assert!(release_proof(&release, &run, "macos").is_some());
        assert!(release_proof(&release, &run, "windows").is_none());
        assert!(release_proof(
            &release,
            &serde_json::json!({"id":7,"run_attempt":3,"head_sha":"source"}),
            "macos"
        )
        .is_none());
        release["draft"] = true.into();
        assert!(release_proof(&release, &run, "macos").is_none());
    }
}

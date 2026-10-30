use tauri::AppHandle;

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

// 软件版本沿用三个无前导零的整数分量，与产品交付版本使用相同数值顺序。
fn version_parts(version: &str) -> Result<[u64; 3], String> {
    let parts: Vec<_> = version.split('.').collect();
    if parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || part.len() > 1 && part.starts_with('0')
        })
    {
        return Err("安装包版本无效".into());
    }
    let numbers = parts
        .iter()
        .map(|part| part.parse::<u64>().map_err(|_| "安装包版本越界".to_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    numbers.try_into().map_err(|_| "安装包版本无效".to_owned())
}

// GitHub传输文件名使用ASCII，中文显示名仍由同一平台和成功版本生成。
fn installer_name(platform: &str, version: &str) -> Result<String, String> {
    let suffix = match platform {
        "macOS" => "dmg",
        "Windows" => "exe",
        "LinuxARM" | "LinuxAMD" => "deb",
        _ => return Err("安装包平台无效".into()),
    };
    version_parts(version)?;
    Ok(format!("citizenchain-{platform}-v{version}.{suffix}"))
}

fn installer_label(platform: &str, version: &str) -> Result<String, String> {
    let filename = installer_name(platform, version)?;
    let suffix = filename.rsplit('.').next().ok_or("安装包后缀无效")?;
    Ok(format!("公民链·{platform}·v{version}.{suffix}"))
}

fn installer_url(tag: &str, filename: &str) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse("https://github.com/crcfrcn/citizenchain/releases/download/")
        .map_err(|_| "更新地址无效")?;
    url.path_segments_mut()
        .map_err(|_| "更新地址路径无效")?
        .push(tag)
        .push(filename);
    Ok(url)
}

// 沿用Release资产证明，只接受该平台唯一完整安装包，不读取额外下载清单。
fn installer_asset_url(
    release: &serde_json::Value,
    proof: &serde_json::Value,
    platform: &str,
    version: &str,
    tag: &str,
) -> Result<String, String> {
    let filename = installer_name(platform, version)?;
    let label = installer_label(platform, version)?;
    let assets = release["assets"].as_array().ok_or("安装包资产列表无效")?;
    let proofs = proof["assets"].as_array().ok_or("安装包资产证明无效")?;
    if assets.len() != 1 || proofs.len() != 1 {
        return Err("本平台必须只有一个完整安装包".into());
    }
    let asset = &assets[0];
    let item = &proofs[0];
    let digest = format!(
        "sha256:{}",
        item["sha256"].as_str().ok_or("安装包摘要无效")?
    );
    let expected = installer_url(tag, &filename)?;
    let actual = asset["browser_download_url"]
        .as_str()
        .ok_or("安装包下载地址缺失")?
        .parse::<reqwest::Url>()
        .map_err(|_| "安装包下载地址无效")?;
    if asset["name"] != filename
        || item["name"] != filename
        || asset["label"] != label
        || item["label"] != label
        || asset["state"] != "uploaded"
        || asset["size"] != item["size"]
        || asset["digest"] != digest
        || actual != expected
    {
        return Err("安装包与本平台Release证明不符".into());
    }
    Ok(expected.into())
}

// URL作为单个参数传给系统浏览器入口，不执行Shell脚本或安装器。
fn installer_browser_command(os: &str, url: &str) -> Result<std::process::Command, String> {
    let mut command = match os {
        "macos" => std::process::Command::new("/usr/bin/open"),
        "windows" => {
            let mut command = std::process::Command::new("rundll32.exe");
            command.arg("url.dll,FileProtocolHandler");
            command
        }
        "linux" => std::process::Command::new("xdg-open"),
        _ => return Err("当前平台没有安装包下载入口".into()),
    };
    command.arg(url);
    Ok(command)
}

// Tag作为一个URL路径段交给现有URL实现编码，避免中文名称或分隔符改变API目标。
fn github_tag_path(tag: &str) -> Result<String, String> {
    let mut url =
        reqwest::Url::parse("https://api.github.com/repos/crcfrcn/citizenchain/git/ref/tags/")
            .map_err(|_| "更新Tag地址无效")?;
    url.path_segments_mut()
        .map_err(|_| "更新Tag路径无效")?
        .pop_if_empty()
        .push(tag);
    url.path()
        .strip_prefix("/repos/crcfrcn/citizenchain/")
        .map(str::to_owned)
        .ok_or_else(|| "更新Tag目标无效".to_owned())
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
pub async fn check_desktop_update(app: AppHandle) -> Result<Option<serde_json::Value>, String> {
    let current_version = app.package_info().version.to_string();
    let (platform, display) = update_target()?;
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
    let version = proof["version"].as_str().ok_or("更新证明版本缺失")?;
    let expected_tag = format!("公民链·{display}·v{version}");
    if tag != expected_tag {
        return Err("更新Tag无效".into());
    }
    let reference = github_json(&client, &github_tag_path(tag)?).await?;
    if reference["object"]["type"] != "commit" || reference["object"]["sha"] != run["head_sha"] {
        return Err("更新Tag提交不符".into());
    }
    let download_url = installer_asset_url(release, proof, display, version, tag)?;
    if version_parts(version)? <= version_parts(&current_version)? {
        return Ok(None);
    }
    Ok(Some(serde_json::json!({
        "currentVersion": current_version, "version": version, "downloadUrl": download_url,
    })))
}

/// 点击时重新发现本平台正式完整包；前端不能指定任意下载或执行目标。
#[tauri::command]
pub async fn open_desktop_installer(app: AppHandle) -> Result<(), String> {
    let package = check_desktop_update(app)
        .await?
        .ok_or("当前没有新版安装包")?;
    let url = package["downloadUrl"]
        .as_str()
        .ok_or("安装包下载地址缺失")?
        .to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let status = installer_browser_command(std::env::consts::OS, &url)?
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|error| format!("打开安装包下载失败：{error}"))?;
        if !status.success() {
            return Err(format!("系统下载入口失败：{status}"));
        }
        Ok(())
    })
    .await
    .map_err(|error| format!("打开安装包下载任务失败：{error}"))?
}
#[cfg(test)]
#[path = "../tests/update.rs"]
mod update_tests;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tauri::{ipc::Channel, AppHandle, Manager};

// 当前平台只读取自己的正式Release，不使用跨平台latest或安装时的旧Tag。
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
        // 基址末尾的空段必须先移除，避免download后出现双斜线。
        .pop_if_empty()
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
    let proof = automation_proof(release)?;
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

// Release发布时间决定最新版本；Run仅用于确认该正式版确实由本平台自动化成功发布。
fn formal_releases(mut releases: Vec<serde_json::Value>, display: &str) -> Vec<serde_json::Value> {
    let prefix = format!("公民链·{display}·v");
    releases.retain(|release| {
        release["draft"] == false
            && release["prerelease"] == false
            && release["tag_name"]
                .as_str()
                .is_some_and(|tag| tag.starts_with(&prefix))
    });
    releases.sort_by(|a, b| {
        b["published_at"]
            .as_str()
            .cmp(&a["published_at"].as_str())
            .then_with(|| b["id"].as_u64().cmp(&a["id"].as_u64()))
    });
    releases
}

fn automation_proof(release: &serde_json::Value) -> Option<serde_json::Value> {
    let entries = release["body"]
        .as_str()?
        .lines()
        .filter_map(|line| line.strip_prefix("<!-- automation:")?.strip_suffix(" -->"))
        .collect::<Vec<_>>();
    if entries.len() != 1 {
        return None;
    }
    serde_json::from_str(entries[0]).ok()
}

/// 仅从当前平台正式Release列表选取最新成功版本，不下载源码或Actions Artifact。
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
    let mut releases = Vec::new();
    for page in 1.. {
        let value = github_json(&client, &format!("releases?per_page=100&page={page}")).await?;
        let rows = value.as_array().ok_or("更新Release列表无效")?;
        releases.extend(rows.iter().cloned());
        if rows.len() < 100 {
            break;
        }
    }
    let mut selected = None;
    for release in formal_releases(releases, display) {
        let Some(proof) = automation_proof(&release) else {
            continue;
        };
        if proof["platform"] != platform {
            continue;
        }
        let run_id = proof["run_id"].as_u64().ok_or("正式Release缺少Run编号")?;
        let run = github_json(&client, &format!("actions/runs/{run_id}")).await?;
        if run["status"] != "completed" || run["conclusion"] != "success" {
            continue;
        }
        if run["path"] != format!(".github/workflows/{workflow}")
            || run["head_branch"] != "main"
            || run["event"] != "workflow_dispatch"
            || run["repository"]["full_name"] != "crcfrcn/citizenchain"
        {
            return Err("正式Release的自动化身份不符".into());
        }
        let proof = release_proof(&release, &run, platform).ok_or("正式Release与成功自动化不符")?;
        selected = Some((release, proof, run));
        break;
    }
    let Some((release, proof, run)) = selected else {
        return Ok(None);
    };
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
    let download_url = installer_asset_url(&release, &proof, display, version, tag)?;
    if version_parts(version)? <= version_parts(&current_version)? {
        return Ok(None);
    }
    Ok(Some(serde_json::json!({
        "currentVersion": current_version, "version": version, "downloadUrl": download_url,
        "releaseId": release["id"], "assetId": release["assets"][0]["id"],
        "size": release["assets"][0]["size"], "filename": installer_name(display, version)?,
    })))
}

// 下载状态归后端；前端不能指定地址、执行路径或安装参数。
#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    status: String,
    version: Option<String>,
    downloaded: u64,
    total: u64,
    error: Option<String>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DownloadedPackage {
    version: String,
    filename: String,
    size: u64,
}

#[derive(Default)]
struct UpdateState {
    progress: UpdateProgress,
    package: Option<DownloadedPackage>,
}
static UPDATE: OnceLock<Mutex<UpdateState>> = OnceLock::new();
fn update_state() -> &'static Mutex<UpdateState> {
    UPDATE.get_or_init(|| Mutex::new(UpdateState::default()))
}
fn update_directory(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("updates");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    Ok(directory)
}
fn progress(status: &str, version: Option<String>, downloaded: u64, total: u64) -> UpdateProgress {
    UpdateProgress {
        status: status.into(),
        version,
        downloaded,
        total,
        error: None,
    }
}
fn publish_progress(
    value: UpdateProgress,
    channel: &Channel<UpdateProgress>,
) -> Result<(), String> {
    update_state()
        .lock()
        .map_err(|_| "更新状态不可用")?
        .progress = value.clone();
    let _ = channel.send(value); // 页面关闭或切换不取消后端下载。
    Ok(())
}

/// 重开应用时恢复完整包；只有完整下载的包才允许进入安装状态。
#[tauri::command]
pub fn desktop_update_status(app: AppHandle) -> Result<UpdateProgress, String> {
    let directory = update_directory(&app)?;
    let mut state = update_state().lock().map_err(|_| "更新状态不可用")?;
    if !state.progress.status.is_empty() {
        return Ok(state.progress.clone());
    }
    state.progress.status = "idle".into();
    if let Ok(bytes) = std::fs::read(directory.join("ready.json")) {
        if let Ok(package) = serde_json::from_slice::<DownloadedPackage>(&bytes) {
            let (_, display) = update_target()?;
            if installer_name(display, &package.version)? == package.filename
                && version_parts(&package.version)?
                    > version_parts(&app.package_info().version.to_string())?
                && std::fs::metadata(directory.join(&package.filename))
                    .is_ok_and(|m| m.is_file() && m.len() == package.size)
            {
                state.progress = progress(
                    "ready",
                    Some(package.version.clone()),
                    package.size,
                    package.size,
                );
                if let Ok(error) = std::fs::read_to_string(directory.join("install-error.txt")) {
                    state.progress.error = Some(error);
                }
                state.package = Some(package);
            }
        }
    }
    Ok(state.progress.clone())
}

fn release_download_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(
            url.host_str(),
            Some(
                "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
            )
        )
}

#[tauri::command]
pub async fn download_desktop_update(
    app: AppHandle,
    on_progress: Channel<UpdateProgress>,
) -> Result<(), String> {
    {
        let mut state = update_state().lock().map_err(|_| "更新状态不可用")?;
        if matches!(
            state.progress.status.as_str(),
            "downloading" | "ready" | "installing"
        ) {
            return Err("更新正在下载或已准备安装".into());
        }
        state.progress = progress("downloading", None, 0, 0);
    }
    let result = download_package(&app, &on_progress).await;
    if let Err(error) = &result {
        let mut value = update_state()
            .lock()
            .map_err(|_| "更新状态不可用")?
            .progress
            .clone();
        value.status = "error".into();
        value.error = Some(error.clone());
        publish_progress(value, &on_progress)?;
    }
    result
}

// 不完整的下载始终保持partial，不向安装阶段交付文件。
fn finish_download(
    file: std::fs::File,
    partial: &Path,
    destination: &Path,
    downloaded: u64,
    total: u64,
) -> Result<(), String> {
    if downloaded != total || file.metadata().map_err(|e| e.to_string())?.len() != total {
        return Err("安装包未下载完整，请重试".into());
    }
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    if destination.exists() {
        std::fs::remove_file(destination).map_err(|e| e.to_string())?;
    }
    std::fs::rename(partial, destination).map_err(|e| e.to_string())
}

async fn download_package(
    app: &AppHandle,
    channel: &Channel<UpdateProgress>,
) -> Result<(), String> {
    let metadata = check_desktop_update(app.clone())
        .await?
        .ok_or("当前没有新版安装包")?;
    let package = DownloadedPackage {
        version: metadata["version"].as_str().ok_or("更新版本缺失")?.into(),
        filename: metadata["filename"]
            .as_str()
            .ok_or("更新文件名缺失")?
            .into(),
        size: metadata["size"]
            .as_u64()
            .filter(|v| *v > 0)
            .ok_or("更新大小无效")?,
    };
    let directory = update_directory(app)?;
    let partial = directory.join(format!(
        "{}.partial-{}",
        package.filename,
        std::process::id()
    ));
    let destination = directory.join(&package.filename);
    let client = reqwest::Client::builder()
        .user_agent("CitizenChain")
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || !release_download_url(attempt.url()) {
                attempt.error("安装包重定向地址无效")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|_| "下载客户端初始化失败")?;
    let result: Result<(), String> = async {
        let mut response = client
            .get(metadata["downloadUrl"].as_str().ok_or("更新地址缺失")?)
            .send()
            .await
            .map_err(|_| "下载连接失败，请重试")?
            .error_for_status()
            .map_err(|_| "安装包下载失败，请重试")?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)
            .map_err(|e| e.to_string())?;
        let mut downloaded = 0;
        let mut last_progress = std::time::Instant::now();
        publish_progress(
            progress(
                "downloading",
                Some(package.version.clone()),
                0,
                package.size,
            ),
            channel,
        )?;
        while let Some(bytes) = response
            .chunk()
            .await
            .map_err(|_| "安装包传输中断，请重试")?
        {
            downloaded += bytes.len() as u64;
            if downloaded > package.size {
                return Err("安装包传输大小不符".into());
            }
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            if downloaded == package.size
                || last_progress.elapsed() >= std::time::Duration::from_millis(100)
            {
                publish_progress(
                    progress(
                        "downloading",
                        Some(package.version.clone()),
                        downloaded,
                        package.size,
                    ),
                    channel,
                )?;
                last_progress = std::time::Instant::now();
            }
        }
        finish_download(file, &partial, &destination, downloaded, package.size)?;
        std::fs::write(
            directory.join("ready.json"),
            serde_json::to_vec(&package).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(directory.join("install-error.txt"));
        update_state().lock().map_err(|_| "更新状态不可用")?.package = Some(package.clone());
        publish_progress(
            progress(
                "ready",
                Some(package.version.clone()),
                downloaded,
                package.size,
            ),
            channel,
        )
    }
    .await;
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

// 更新进程只接受后端固定的完整包和当前安装路径，等待原PID完全退出后才修改程序。
fn installation_script(
    os: &str,
    directory: &Path,
    package: &Path,
    executable: &Path,
    pid: u32,
) -> Result<String, String> {
    let text = |p: &Path| {
        p.to_str()
            .map(str::to_owned)
            .ok_or("更新路径不是有效文本".to_owned())
    };
    let directory = text(directory)?;
    let package = text(package)?;
    let executable = text(executable)?;
    if os == "windows" {
        let q = powershell_quote;
        return Ok(format!(
            r#"$ErrorActionPreference='Stop'
$root={root}; $package={package}; $exe={exe}
Set-Content -LiteralPath (Join-Path $root 'started') -Value 'ready'
$oldExited=$false
try {{
  $old=Get-Process -Id {pid} -ErrorAction SilentlyContinue
  if($old -and !$old.WaitForExit(300000)) {{ throw '旧应用未完全退出，未执行安装' }}
  $oldExited=$true
  $installer=Start-Process -FilePath $package -ArgumentList ('/S /D=' + (Split-Path -Parent $exe)) -Verb RunAs -PassThru -Wait
  if($installer.ExitCode -ne 0) {{ throw "安装失败，退出码 $($installer.ExitCode)" }}
  Start-Process -FilePath $exe
  Remove-Item -LiteralPath $package,(Join-Path $root 'ready.json'),(Join-Path $root 'started') -ErrorAction SilentlyContinue
}} catch {{
  Set-Content -LiteralPath (Join-Path $root 'install-error.txt') -Value $_.Exception.Message -Encoding UTF8
  if($oldExited) {{ Start-Process -FilePath $exe }}
}}
"#,
            root = q(&directory),
            package = q(&package),
            exe = q(&executable)
        ));
    }
    let q = shell_quote;
    let mut script = format!(
        r#"#!/bin/sh
root={root}
package={package}
exe={exe}
exec >> "$root/install.log" 2>&1
printf ready > "$root/started"
count=0
while kill -0 {pid} 2>/dev/null; do
  count=$((count + 1))
  if [ "$count" -ge 300 ]; then printf '旧应用未完全退出，未执行安装' > "$root/install-error.txt"; exit 1; fi
  sleep 1
done
"#,
        root = q(&directory),
        package = q(&package),
        exe = q(&executable)
    );
    match os {
        "macos" => {
            let destination = Path::new(&executable)
                .ancestors()
                .find(|p| p.extension().is_some_and(|e| e == "app"))
                .ok_or("当前程序不在已安装的App中，不能自动替换")?;
            let destination = text(destination)?;
            // 保留现有应用实际名称及位置；失败恢复旧应用，不触碰链数据或设置。
            script.push_str(&format!(
                r#"destination={destination}
mount="$root/mount"
stage="$destination.update-{pid}"
backup="$destination.previous-{pid}"
fail() {{ printf '%s' "$1" > "$root/install-error.txt"; /usr/bin/open "$destination"; exit 1; }}
mkdir -p "$mount" || fail '无法准备安装目录'
/usr/bin/hdiutil attach -readonly -nobrowse -mountpoint "$mount" "$package" || fail '安装包挂载失败'
trap '/usr/bin/hdiutil detach "$mount" >/dev/null 2>&1' EXIT
set -- "$mount"/*.app
[ "$#" -eq 1 ] && [ -d "$1" ] || fail '安装包中没有唯一应用'
source_app="$1"
"#,
                destination = q(&destination)
            ));
            // 路径可能包含引号，替换脚本通过单独参数读取，不把路径拼进管理员命令。
            let replace = r#"#!/bin/sh
mode=$1; source_app=$2; stage=$3; destination=$4; backup=$5
case "$mode" in
install)
  [ ! -e "$stage" ] && [ ! -e "$backup" ] || exit 1
  /usr/bin/ditto "$source_app" "$stage" || { rm -rf "$stage"; exit 1; }
  mv "$destination" "$backup" || { rm -rf "$stage"; exit 1; }
  if ! mv "$stage" "$destination"; then mv "$backup" "$destination"; rm -rf "$stage"; exit 1; fi
  ;;
rollback)
  mv "$destination" "$stage" && mv "$backup" "$destination" && rm -rf "$stage"
  ;;
cleanup) rm -rf "$backup" ;;
*) exit 1 ;;
esac
"#;
            script.push_str(&format!(
                r#"cat > "$root/replace.sh" <<'CITIZENCHAIN_REPLACE'
{replace}CITIZENCHAIN_REPLACE
"#
            ));
            script.push_str(r#"replace_app() {
  if [ -w "$(dirname "$destination")" ]; then
    /bin/sh "$root/replace.sh" "$1" "$source_app" "$stage" "$destination" "$backup"
  else
    # 使用系统管理员授权，不绕过权限；路径由AppleScript逐项转义。
    /usr/bin/osascript - "$root/replace.sh" "$1" "$source_app" "$stage" "$destination" "$backup" <<'CITIZENCHAIN_ADMIN'
on run argv
  set cmd to "/bin/sh"
  repeat with arg in argv
    set cmd to cmd & " " & quoted form of arg
  end repeat
  do shell script cmd with administrator privileges
end run
CITIZENCHAIN_ADMIN
  fi
}
replace_app install || fail '安装新版失败，已保留旧应用'
if ! /usr/bin/open "$destination"; then
  replace_app rollback
  fail '新版启动失败，已恢复旧应用'
fi
replace_app cleanup
/usr/bin/hdiutil detach "$mount" >/dev/null 2>&1
trap - EXIT
rmdir "$mount" 2>/dev/null
rm -f "$package" "$root/ready.json" "$root/started" "$root/replace.sh" "$root/install.sh" "$root/install.log"
"#);
        }
        "linux" => {
            script.push_str(
                r#"if /usr/bin/pkexec /usr/bin/dpkg --install "$package"; then
  rm -f "$package" "$root/ready.json" "$root/started" "$root/install.sh" "$root/install.log"
else
  printf '安装被取消或安装失败，请重试' > "$root/install-error.txt"
fi
"$exe" >/dev/null 2>&1 &
"#,
            );
        }
        _ => return Err("当前平台没有自动安装实现".into()),
    }
    Ok(script)
}

/// 第二次明确点击才交接安装。现有Exit回调负责停止节点及释放数据库锁。
#[tauri::command]
pub async fn install_desktop_update(app: AppHandle) -> Result<(), String> {
    let directory = update_directory(&app)?;
    let package = {
        let mut state = update_state().lock().map_err(|_| "更新状态不可用")?;
        if state.progress.status != "ready" {
            return Err("请先完整下载新版安装包".into());
        }
        let package = state.package.clone().ok_or("更新包状态缺失")?;
        state.progress.status = "installing".into();
        package
    };
    let path = directory.join(&package.filename);
    if !std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() == package.size) {
        let error = "安装包已变化，请重新下载".to_owned();
        let mut state = update_state().lock().map_err(|_| "更新状态不可用")?;
        state.progress.status = "error".into();
        state.progress.error = Some(error.clone());
        state.package = None;
        return Err(error);
    }
    let result: Result<(), String> = async {
        if std::env::consts::OS == "linux" && !Path::new("/usr/bin/pkexec").is_file() {
            return Err("系统缺少安装授权入口pkexec，未退出当前程序".into());
        }
        let script = installation_script(
            std::env::consts::OS,
            &directory,
            &path,
            &std::env::current_exe().map_err(|e| e.to_string())?,
            std::process::id(),
        )?;
        let started = directory.join("started");
        let _ = std::fs::remove_file(&started);
        let _ = std::fs::remove_file(directory.join("install-error.txt"));
        let mut command;
        if cfg!(target_os = "windows") {
            command = std::process::Command::new("powershell.exe");
            command.args(["-NoProfile", "-Command", &script]);
        } else {
            let helper = directory.join("install.sh");
            std::fs::write(&helper, script).map_err(|e| e.to_string())?;
            command = std::process::Command::new("/bin/sh");
            command.arg(helper);
        }
        let mut child = command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("更新进程启动失败：{e}"))?;
        for _ in 0..100 {
            if started.exists() {
                return Ok(());
            }
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                return Err("更新进程启动失败，当前程序继续运行".into());
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let _ = child.kill();
        let _ = child.wait();
        Err("更新进程未就绪，当前程序继续运行".into())
    }
    .await;
    if let Err(error) = result {
        let mut state = update_state().lock().map_err(|_| "更新状态不可用")?;
        state.progress.status = "ready".into();
        state.progress.error = Some(error.clone());
        return Err(error);
    }
    app.exit(0);
    Ok(())
}

#[cfg(test)]
#[path = "../tests/update.rs"]
mod update_tests;

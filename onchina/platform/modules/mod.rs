//! 模块说明由实际入口提供，平台只装载、转发和管理进程。
#[path = "capability.rs"]
pub mod capability;
#[path = "model.rs"]
pub mod model;
use crate::{api_error, AppState};
use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use model::{
    ModuleBusiness, ModuleDescription, ModuleRoute, RouteDescription, WorkspaceDeclaration,
    WorkspaceKind, WorkspaceSection,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    net::{Ipv4Addr, TcpListener},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// 版本及业务声明来自模块入口；不按模块名硬编码业务分支。
pub fn describe_module(
    id: &str,
    version: &str,
    frontend_source: &str,
    exports: &[&str],
    routes: Vec<ModuleRoute>,
    purposes: Vec<String>,
    actions: Vec<String>,
    declaration: fn(
        &str,
    ) -> Option<(
        capability::CapabilitySet,
        WorkspaceKind,
        Vec<WorkspaceSection>,
    )>,
) -> ModuleDescription {
    let mut capabilities = BTreeMap::new();
    let mut workspaces = BTreeMap::new();
    for info in primitives::cid::code::INSTITUTION_CODE_INFOS {
        let code = info.institution_code_text;
        if let Some((caps, kind, sections)) = declaration(code) {
            capabilities.insert(code.into(), caps);
            workspaces.insert(code.into(), WorkspaceDeclaration { kind, sections });
        }
    }
    ModuleDescription {
        id: id.into(),
        version: version.into(),
        interface_version: model::MODULE_INTERFACE_VERSION,
        frontend_source: frontend_source.into(),
        frontend_exports: exports.iter().map(|v| (*v).into()).collect(),
        frontend_workspace: None,
        instance_workspace: false,
        routes: routes.into_iter().map(|v| v.description).collect(),
        purposes,
        actions,
        capabilities,
        workspaces,
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModuleAsset {
    pub(crate) id: String,
    pub(crate) version: String,
    pub(crate) interface_version: u32,
    pub(crate) script: String,
    pub(crate) stylesheet: String,
    pub(crate) exports: Vec<String>,
    pub(crate) institution_codes: Vec<String>,
    pub(crate) workspace: Option<String>,
}
// 启动中或异步任务取消时仍持有进程所有权，不能让尚未登记的模块成为孤儿。
struct ModuleProcess(Child);
impl std::ops::Deref for ModuleProcess {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}
impl std::ops::DerefMut for ModuleProcess {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}
impl Drop for ModuleProcess {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            if self.0.kill().is_ok() {
                let _ = self.0.wait();
            }
        }
    }
}
struct PreparedModule {
    description: ModuleDescription,
    executable: PathBuf,
    frontend: PathBuf,
}
const MODULE_CONCURRENCY: usize = 4;
const WORKSPACE_TIMEOUT: Duration = Duration::from_secs(5);
struct RemoteModule {
    description: ModuleDescription,
    client: reqwest::Client,
    endpoint: String,
    frontend: PathBuf,
    child: Mutex<ModuleProcess>,
}
impl RemoteModule {
    fn available(&self) -> bool {
        self.child
            .lock()
            .ok()
            .is_some_and(|mut child| matches!(child.try_wait(), Ok(None)))
    }
}
/// 一次启动对应一个目录；故障或缺失模块不会删除其会话及业务数据。
pub struct ModuleCatalog {
    pub local: Option<Arc<dyn ModuleBusiness>>,
    remotes: Mutex<Vec<Arc<RemoteModule>>>,
    local_description: Option<ModuleDescription>,
}
impl ModuleCatalog {
    pub fn new(local: Option<Arc<dyn ModuleBusiness>>) -> Self {
        let local_description = local.as_ref().map(|module| module.describe());
        Self {
            local,
            local_description,
            remotes: Mutex::new(Vec::new()),
        }
    }
    fn descriptions(&self) -> Vec<ModuleDescription> {
        if let Some(description) = &self.local_description {
            return vec![description.clone()];
        }
        self.remotes
            .lock()
            .map(|remotes| {
                remotes
                    .iter()
                    .filter(|m| m.available())
                    .map(|m| m.description.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn capabilities(&self, code: &str) -> Option<capability::CapabilitySet> {
        self.descriptions()
            .iter()
            .find_map(|m| m.capabilities.get(code).copied())
    }
    pub fn workspace(&self, code: &str) -> Option<WorkspaceDeclaration> {
        self.descriptions()
            .iter()
            .find_map(|m| m.workspaces.get(code).cloned())
    }
    pub(crate) fn owner_of_purpose(&self, purpose: &str) -> Option<String> {
        self.descriptions()
            .iter()
            .find(|m| m.purposes.iter().any(|p| p == purpose))
            .map(|m| m.id.clone())
    }
    pub(crate) fn owner_of_action(&self, action: &str) -> Option<String> {
        self.descriptions()
            .iter()
            .find(|m| m.actions.iter().any(|p| p == action))
            .map(|m| m.id.clone())
    }
    pub(crate) fn local_description(&self) -> Option<&ModuleDescription> {
        self.local_description.as_ref()
    }
    pub(crate) fn local_owns(&self, owner: &str) -> bool {
        self.local_description
            .as_ref()
            .is_some_and(|m| m.id == owner)
    }
    pub(crate) fn local_purposes(&self) -> Vec<String> {
        self.local_description
            .as_ref()
            .map(|m| m.purposes.clone())
            .unwrap_or_default()
    }
    pub(crate) fn assets(&self) -> Vec<ModuleAsset> {
        self.descriptions()
            .into_iter()
            .map(|m| ModuleAsset {
                script: format!("/modules/{}/module.js?v={}", m.id, m.version),
                stylesheet: format!("/modules/{}/module.css?v={}", m.id, m.version),
                institution_codes: m.workspaces.keys().cloned().collect(),
                workspace: m.frontend_workspace,
                id: m.id,
                version: m.version,
                interface_version: m.interface_version,
                exports: m.frontend_exports,
            })
            .collect()
    }
    pub fn routes(&self) -> Vec<(String, RouteDescription)> {
        self.descriptions()
            .into_iter()
            .flat_map(|m| {
                let id = m.id;
                m.routes.into_iter().map(move |r| (id.clone(), r))
            })
            .collect()
    }
    pub(crate) fn static_frontends(&self) -> Vec<(String, PathBuf)> {
        self.remotes
            .lock()
            .map(|list| {
                list.iter()
                    .filter(|m| m.available())
                    .map(|m| (m.description.id.clone(), m.frontend.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }
    fn remote(&self, id: &str) -> Option<Arc<RemoteModule>> {
        self.remotes
            .lock()
            .ok()?
            .iter()
            .find(|m| m.description.id == id && m.available())
            .cloned()
    }
    pub(crate) async fn workspace_modules(&self, cid: &str, headers: &HeaderMap) -> Vec<String> {
        if let Some(local) = &self.local {
            return local.workspace_modules(cid).await;
        }
        let mut pending = tokio::task::JoinSet::new();
        let permits = Arc::new(tokio::sync::Semaphore::new(MODULE_CONCURRENCY));
        for (index, description) in self
            .descriptions()
            .into_iter()
            .filter(|m| m.instance_workspace)
            .enumerate()
        {
            let Some(remote) = self.remote(&description.id) else {
                continue;
            };
            let headers = headers.clone();
            let permits = permits.clone();
            pending.spawn(async move {
                let _permit = permits.acquire_owned().await.map_err(|e| e.to_string())?;
                // 只限制内部只读查询；正式交易转发仍保留原有等待语义。
                let response = remote
                    .client
                    .get(format!("{}/internal/workspace", remote.endpoint))
                    .headers(headers)
                    .timeout(WORKSPACE_TIMEOUT)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
                if !response.status().is_success() {
                    return Err("module workspace unavailable".to_string());
                }
                let value: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
                let items = value
                    .get("data")
                    .and_then(|v| v.as_array())
                    .ok_or("invalid module workspace")?
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect::<Vec<_>>();
                Ok::<_, String>((index, items))
            });
        }
        let mut ordered = BTreeMap::new();
        while let Some(result) = pending.join_next().await {
            match result {
                Ok(Ok((index, items))) => {
                    ordered.insert(index, items);
                }
                Ok(Err(error)) => tracing::warn!(%error, "module workspace unavailable"),
                Err(error) => tracing::warn!(%error, "module workspace task failed"),
            }
        }
        ordered.into_values().flatten().collect()
    }
    pub(crate) async fn load_installed(
        &self,
        database_url: &str,
        reserved: &[RouteDescription],
    ) -> Result<(), String> {
        let directory = module_directory()?;
        if !directory.exists() {
            return Ok(());
        }
        let mut occupied = route_keys(reserved)?;
        let mut purposes: BTreeSet<String> = crate::transactions::handler::builtin_purposes()
            .into_iter()
            .map(str::to_string)
            .collect();
        let mut actions = BTreeSet::from([
            crate::authentication::operation_auth::AdminActionType::NodeBindingUnbind
                .as_str()
                .to_string(),
        ]);
        let mut capabilities = BTreeSet::new();
        let mut descriptions = tokio::task::JoinSet::new();
        let permits = Arc::new(tokio::sync::Semaphore::new(MODULE_CONCURRENCY));
        for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            let permits = permits.clone();
            descriptions.spawn(async move {
                let permit = permits.acquire_owned().await.map_err(|e| e.to_string())?;
                // 同步说明读取放入有界阻塞任务，不能占住HTTPS执行线程。
                let result = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    let result = Self::prepare_one(&path);
                    (path, result)
                })
                .await
                .map_err(|e| e.to_string())?;
                Ok::<_, String>(result)
            });
        }
        let mut ordered = BTreeMap::new();
        while let Some(result) = descriptions.join_next().await {
            match result {
                Ok(Ok((path, result))) => {
                    ordered.insert(path, result);
                }
                Ok(Err(error)) => tracing::error!(%error, "module description task failed"),
                Err(error) => tracing::error!(%error, "module description task failed"),
            }
        }
        let mut pending = tokio::task::JoinSet::new();
        // 归属校验按实际目录稳定排序；耗时就绪等待独立并行，不串行累加120秒。
        for (path, result) in ordered {
            let prepared = match result {
                Ok(prepared) => prepared,
                Err(error) => {
                    tracing::error!(module = %path.display(), %error, "module not loaded");
                    continue;
                }
            };
            let keys = route_keys(&prepared.description.routes)?;
            if !occupied.is_disjoint(&keys)
                || prepared
                    .description
                    .purposes
                    .iter()
                    .any(|v| purposes.contains(v))
                || prepared
                    .description
                    .actions
                    .iter()
                    .any(|v| actions.contains(v))
                || prepared
                    .description
                    .capabilities
                    .keys()
                    .any(|v| capabilities.contains(v))
            {
                tracing::error!(module = %path.display(), "module route or ownership conflict");
                continue;
            }
            occupied.extend(keys);
            purposes.extend(prepared.description.purposes.iter().cloned());
            actions.extend(prepared.description.actions.iter().cloned());
            capabilities.extend(prepared.description.capabilities.keys().cloned());
            let database_url = database_url.to_string();
            let permits = permits.clone();
            pending.spawn(async move {
                let _permit = permits.acquire_owned().await.map_err(|e| e.to_string())?;
                Self::start_one(prepared, &database_url).await
            });
        }
        while let Some(result) = pending.join_next().await {
            match result {
                Ok(Ok(remote)) => self
                    .remotes
                    .lock()
                    .map_err(|_| "module catalog lock poisoned")?
                    .push(Arc::new(remote)),
                Ok(Err(error)) => tracing::error!(%error, "module not loaded"),
                Err(error) => tracing::error!(%error, "module startup task failed"),
            }
        }
        self.remotes
            .lock()
            .map_err(|_| "module catalog lock poisoned")?
            .sort_by(|a, b| a.description.id.cmp(&b.description.id));
        Ok(())
    }
    fn prepare_one(directory: &Path) -> Result<PreparedModule, String> {
        let metadata = std::fs::symlink_metadata(directory).map_err(|e| e.to_string())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("module must be an actual directory".into());
        }
        let id = directory
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or("invalid module directory")?;
        if !valid_id(id) {
            return Err("invalid module id".into());
        }
        let executable = directory.join(format!("onchina-{id}{}", std::env::consts::EXE_SUFFIX));
        let metadata = std::fs::symlink_metadata(&executable).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("module executable must be a regular file".into());
        }
        let description = read_description(&executable)?;
        validate_description(&description)?;
        if description.id != id {
            return Err("module directory and description mismatch".into());
        }
        let frontend = directory.join("frontend");
        verify_frontend(&frontend, &description)?;
        Ok(PreparedModule {
            description,
            executable,
            frontend,
        })
    }
    async fn start_one(
        prepared: PreparedModule,
        database_url: &str,
    ) -> Result<RemoteModule, String> {
        let PreparedModule {
            description,
            executable,
            frontend,
        } = prepared;
        let socket = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
        let address = socket.local_addr().map_err(|e| e.to_string())?;
        drop(socket);
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .resolve(crate::PLATFORM_HOST, address)
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| e.to_string())?;
        let mut child = ModuleProcess(
            Command::new(executable)
                .env("DATABASE_URL", database_url)
                .env("ONCHINA_EMBEDDED_PG", "0")
                .env("ONCHINA_MODULE_BIND", address.to_string())
                .env("ONCHINA_TLS_DIR", crate::tls::tls_dir()?)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .map_err(|e| e.to_string())?,
        );
        let endpoint = format!("https://{}:{}", crate::PLATFORM_HOST, address.port());
        let start = Instant::now();
        loop {
            if child.try_wait().map_err(|e| e.to_string())?.is_some()
                || start.elapsed() > Duration::from_secs(120)
            {
                return Err(format!("module {} did not become ready", description.id));
            }
            if let Ok(response) = client
                .get(format!("{endpoint}/api/health"))
                .timeout(Duration::from_secs(2))
                .send()
                .await
            {
                if let Ok(value) = response.json::<serde_json::Value>().await {
                    if value.pointer("/data/status").and_then(|v| v.as_str()) == Some("UP")
                        && value.pointer("/data/module_id").and_then(|v| v.as_str())
                            == Some(description.id.as_str())
                        && value
                            .pointer("/data/module_version")
                            .and_then(|v| v.as_str())
                            == Some(description.version.as_str())
                    {
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(RemoteModule {
            description,
            client,
            endpoint,
            frontend,
            child: Mutex::new(child),
        })
    }
    pub(crate) fn stop_all(&self) -> Result<(), String> {
        let remotes = self
            .remotes
            .lock()
            .map_err(|_| "module catalog lock poisoned")?;
        for remote in remotes.iter() {
            let mut child = remote
                .child
                .lock()
                .map_err(|_| "module process lock poisoned")?;
            drop(child.stdin.take());
        }
        let start = Instant::now();
        for remote in remotes.iter() {
            let mut child = remote
                .child
                .lock()
                .map_err(|_| "module process lock poisoned")?;
            while child.try_wait().map_err(|e| e.to_string())?.is_none()
                && start.elapsed() < Duration::from_secs(12)
            {
                std::thread::sleep(Duration::from_millis(25));
            }
            if child.try_wait().map_err(|e| e.to_string())?.is_none() {
                child.kill().map_err(|e| e.to_string())?;
            }
            child.wait().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

pub(crate) fn module_directory() -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("ONCHINA_MODULE_DIR") {
        let path = PathBuf::from(value);
        if !path.is_absolute() {
            return Err("ONCHINA_MODULE_DIR must be absolute".into());
        }
        return Ok(path);
    }
    if cfg!(target_os = "macos") {
        return Ok(
            PathBuf::from(std::env::var_os("HOME").ok_or("HOME unavailable")?)
                .join("Library/Application Support/gmb.dev/onchina-modules"),
        );
    }
    Ok(std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("executable directory missing")?
        .join("modules"))
}
pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|v| v.is_ascii_lowercase() || v.is_ascii_digit() || v == b'_')
        && id.as_bytes()[0].is_ascii_lowercase()
}
fn valid_version(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|v| v.is_ascii_digit()))
}
pub(crate) fn validate_description(value: &ModuleDescription) -> Result<(), String> {
    if !valid_id(&value.id)
        || value.interface_version != model::MODULE_INTERFACE_VERSION
        || !valid_version(&value.version)
    {
        return Err("unsupported module identity/version/interface".into());
    }
    let source = Path::new(&value.frontend_source);
    if source.is_absolute()
        || source
            .components()
            .any(|v| !matches!(v, std::path::Component::Normal(_)))
        || !value.frontend_source.starts_with("frontend/")
    {
        return Err("frontend source must stay inside its module".into());
    }
    if value.frontend_exports.is_empty()
        || value
            .frontend_exports
            .iter()
            .any(|v| v.is_empty() || !v.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'))
    {
        return Err("invalid frontend exports".into());
    }
    if value.frontend_exports.iter().collect::<BTreeSet<_>>().len() != value.frontend_exports.len()
        || value
            .frontend_workspace
            .as_ref()
            .is_some_and(|name| !value.frontend_exports.contains(name))
    {
        return Err("invalid frontend workspace or duplicate exports".into());
    }
    if value
        .capabilities
        .keys()
        .chain(value.workspaces.keys())
        .any(|code| primitives::cid::code::institution_code_from_str(code).is_none())
    {
        return Err("unknown institution capability code".into());
    }
    if !value.capabilities.keys().eq(value.workspaces.keys()) {
        return Err("module capability and workspace ownership mismatch".into());
    }
    for action in &value.actions {
        let action = crate::authentication::operation_auth::parse_action_type(action)
            .map_err(|_| "invalid module action")?;
        if !action.requires_module() {
            return Err("module cannot replace a platform action".into());
        }
    }
    route_keys(&value.routes)?;
    for values in [&value.purposes, &value.actions] {
        if values.iter().collect::<BTreeSet<_>>().len() != values.len()
            || values.iter().any(|v| v.is_empty())
        {
            return Err("duplicate module ownership".into());
        }
    }
    Ok(())
}
fn route_keys(routes: &[RouteDescription]) -> Result<BTreeSet<(String, String)>, String> {
    let mut keys = BTreeSet::new();
    for route in routes {
        if !matches!(route.group.as_str(), "auth" | "admin" | "public" | "app")
            || !route.path.starts_with("/api/")
            || route.methods.is_empty()
            || route.body_limit == 0
            || route.body_limit > crate::documents::MAX_REQUEST_BYTES
        {
            return Err("invalid module route declaration".into());
        }
        let mut pattern = Vec::new();
        for part in route.path.split('/') {
            if part.starts_with('{')
                && part.ends_with('}')
                && part.len() > 2
                && part[1..part.len() - 1]
                    .bytes()
                    .all(|v| v.is_ascii_alphanumeric() || v == b'_')
            {
                pattern.push("{}");
            } else if part
                .bytes()
                .all(|v| v.is_ascii_alphanumeric() || matches!(v, b'-' | b'_' | b'.'))
            {
                pattern.push(part);
            } else {
                return Err("unsupported module route pattern".into());
            }
        }
        let pattern = pattern.join("/");
        let mut methods = BTreeSet::new();
        for method in &route.methods {
            if !matches!(
                method.as_str(),
                "GET" | "POST" | "PATCH" | "PUT" | "DELETE" | "HEAD" | "OPTIONS"
            ) || !methods.insert(method.as_str())
            {
                return Err("duplicate or unsupported route method".into());
            }
        }
        if methods.contains("GET") {
            methods.insert("HEAD");
        }
        for method in methods {
            if !keys.insert((method.into(), pattern.clone())) {
                return Err("module route conflict".into());
            }
        }
    }
    Ok(keys)
}

fn read_description(path: &Path) -> Result<ModuleDescription, String> {
    let mut child = ModuleProcess(
        Command::new(path)
            .arg("--describe")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?,
    );
    let stdout = child
        .stdout
        .take()
        .ok_or("module description pipe missing")?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
            .map_err(|e| e.to_string())
    });
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err("module description timed out".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let bytes = reader
        .join()
        .map_err(|_| "module description reader failed")??;
    if !status.success() || bytes.len() > 1024 * 1024 {
        return Err("module description failed or exceeded limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("invalid module description: {e}"))
}
fn verify_frontend(directory: &Path, module: &ModuleDescription) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(directory).map_err(|e| e.to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("module frontend must be an actual directory".into());
    }
    for name in ["module.js", "module.css"] {
        let script = directory.join(name);
        let metadata = std::fs::symlink_metadata(&script).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("module frontend must be a regular file".into());
        }
        let text = std::fs::read_to_string(script).map_err(|e| e.to_string())?;
        let line = text
            .lines()
            .next()
            .ok_or("module frontend header missing")?;
        let json = line
            .strip_prefix("/*onchina:")
            .and_then(|v| v.strip_suffix("*/"))
            .ok_or("module frontend header invalid")?;
        let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if value["id"] != module.id
            || value["version"] != module.version
            || value["interfaceVersion"] != module.interface_version
        {
            return Err("module backend/frontend versions mismatch".into());
        }
    }
    Ok(())
}
pub(crate) async fn forward_request(
    state: &AppState,
    owner: &str,
    request: Request,
    limit: usize,
) -> Response {
    let Some(remote) = state.modules.remote(owner) else {
        return api_error(StatusCode::SERVICE_UNAVAILABLE, 1503, "module unavailable");
    };
    let (parts, body) = request.into_parts();
    let bytes = match axum::body::to_bytes(body, limit).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return api_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                1001,
                "request body too large",
            )
        }
    };
    let target = format!(
        "{}{}",
        remote.endpoint,
        parts
            .uri
            .path_and_query()
            .map(|v| v.as_str())
            .unwrap_or("/")
    );
    let mut headers = parts.headers;
    for name in [
        "host",
        "connection",
        "transfer-encoding",
        "content-length",
        "upgrade",
        "proxy-authorization",
        "proxy-authenticate",
        "te",
        "trailer",
    ] {
        headers.remove(name);
    }
    match remote
        .client
        .request(parts.method, target)
        .headers(headers)
        .body(bytes)
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            let mut headers = response.headers().clone();
            for name in [
                "connection",
                "transfer-encoding",
                "upgrade",
                "te",
                "trailer",
            ] {
                headers.remove(name);
            }
            match response.bytes().await {
                Ok(bytes) => (status, headers, Body::from(bytes)).into_response(),
                Err(error) => {
                    tracing::warn!(module = %owner, error = %error, "module response failed");
                    api_error(StatusCode::BAD_GATEWAY, 1502, "module response unavailable")
                }
            }
        }
        Err(error) => {
            tracing::warn!(module = %owner, error = %error, "module request failed");
            api_error(StatusCode::BAD_GATEWAY, 1502, "module unavailable")
        }
    }
}
pub(crate) async fn forward_json(
    state: &AppState,
    owner: &str,
    path: &str,
    headers: &HeaderMap,
    value: &impl Serialize,
) -> Response {
    let bytes = match serde_json::to_vec(value) {
        Ok(bytes) => bytes,
        Err(_) => return api_error(StatusCode::BAD_REQUEST, 1001, "invalid request"),
    };
    let mut request = Request::new(Body::from(bytes));
    *request.method_mut() = Method::POST;
    let Ok(uri) = path.parse() else {
        return api_error(StatusCode::BAD_REQUEST, 1001, "invalid route");
    };
    *request.uri_mut() = uri;
    *request.headers_mut() = headers.clone();
    request.headers_mut().insert(
        "content-type",
        axum::http::HeaderValue::from_static("application/json"),
    );
    forward_request(state, owner, request, 2 * 1024 * 1024).await
}
pub(crate) async fn internal_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let ctx = match crate::require_admin_any(&state, &headers) {
        Ok(ctx) => ctx,
        Err(response) => return response,
    };
    let Some(local) = &state.modules.local else {
        return api_error(StatusCode::NOT_FOUND, 1004, "module endpoint unavailable");
    };
    Json(crate::ApiResponse {
        code: 0,
        message: "ok".into(),
        data: local.workspace_modules(&ctx.institution_cid_number).await,
    })
    .into_response()
}

/// 内部只读调用复用刚签发的原会话，不增加登录或扫码步骤。
pub(crate) fn session_headers(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    let value = axum::http::HeaderValue::from_str(&format!("Bearer {token}"))
        .unwrap_or_else(|_| panic!("invalid internal session header"));
    headers.insert("authorization", value);
    headers
}

/// 旧显式维护入口由所属模块执行；启动宿主不调用这些维护命令。
pub(crate) fn run_module_command(
    id: &str,
    command: &str,
    database_url: &str,
) -> Result<(), String> {
    if !valid_id(id) {
        return Err("invalid module id".into());
    }
    let executable = module_directory()?
        .join(id)
        .join(format!("onchina-{id}{}", std::env::consts::EXE_SUFFIX));
    let description = read_description(&executable)?;
    validate_description(&description)?;
    if description.id != id {
        return Err("module identity mismatch".into());
    }
    let status = Command::new(executable)
        .arg(command)
        .env("DATABASE_URL", database_url)
        .env("ONCHINA_EMBEDDED_PG", "0")
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("module maintenance failed".into())
    }
}
#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/modules.rs"]
mod tests;

/// 测试金标只用于输入夹具；各二进制另核验真实模块输出，不能把夹具当生产声明。
#[cfg(any(test, feature = "test-support"))]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/modules_test_support.rs"]
pub mod test_support;

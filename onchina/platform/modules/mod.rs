//! 模块说明由实际入口提供，平台只装载、转发和管理进程。
#[path = "capability.rs"]
pub(crate) mod capability;
#[path = "model.rs"]
pub(crate) mod model;
use std::{collections::{BTreeMap, BTreeSet}, io::Read, net::{Ipv4Addr, TcpListener}, path::{Path, PathBuf}, process::{Child, Command, Stdio}, sync::{Arc, Mutex}, time::{Duration, Instant}};
use axum::{body::Body, extract::{Request, State}, http::{HeaderMap, Method, StatusCode}, response::{IntoResponse, Response}, Json};
use serde::Serialize;
use model::{ModuleBusiness, ModuleDescription, ModuleRoute, RouteDescription, WorkspaceDeclaration, WorkspaceKind, WorkspaceSection};
use crate::{api_error, AppState};

/// 版本及业务声明来自模块入口；不按模块名硬编码业务分支。
pub(crate) fn describe_module(
    id: &str, version: &str, frontend_source: &str, exports: &[&str],
    routes: Vec<ModuleRoute>, purposes: Vec<String>, actions: Vec<String>,
    declaration: fn(&str) -> Option<(capability::CapabilitySet, WorkspaceKind, Vec<WorkspaceSection>)>,
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
        id: id.into(), version: version.into(), interface_version: model::MODULE_INTERFACE_VERSION,
        frontend_source: frontend_source.into(), frontend_exports: exports.iter().map(|v| (*v).into()).collect(),
        instance_workspace: false, routes: routes.into_iter().map(|v| v.description).collect(),
        purposes, actions, capabilities, workspaces,
    }
}
#[derive(Clone, Serialize)]
pub(crate) struct ModuleAsset {
    pub(crate) id: String,
    pub(crate) version: String,
    pub(crate) interface_version: u32,
    pub(crate) script: String,
    pub(crate) stylesheet: String,
    pub(crate) exports: Vec<String>,
    pub(crate) institution_codes: Vec<String>,
}
struct RemoteModule {
    description: ModuleDescription,
    client: reqwest::Client,
    endpoint: String,
    frontend: PathBuf,
    child: Mutex<Child>,
}
impl RemoteModule {
    fn available(&self) -> bool {
        self.child.lock().ok().is_some_and(|mut child| matches!(child.try_wait(), Ok(None)))
    }
}
/// 一次启动对应一个目录；故障或缺失模块不会删除其会话及业务数据。
pub(crate) struct ModuleCatalog {
    pub(crate) local: Option<Arc<dyn ModuleBusiness>>,
    remotes: Mutex<Vec<Arc<RemoteModule>>>,
}
impl ModuleCatalog {
    pub(crate) fn new(local: Option<Arc<dyn ModuleBusiness>>) -> Self { Self { local, remotes: Mutex::new(Vec::new()) } }
    fn descriptions(&self) -> Vec<ModuleDescription> {
        if let Some(local) = &self.local { return vec![local.describe()]; }
        self.remotes.lock().map(|remotes| remotes.iter().filter(|m| m.available()).map(|m| m.description.clone()).collect()).unwrap_or_default()
    }
    pub(crate) fn capabilities(&self, code: &str) -> Option<capability::CapabilitySet> {
        self.descriptions().iter().find_map(|m| m.capabilities.get(code).copied())
    }
    pub(crate) fn workspace(&self, code: &str) -> Option<WorkspaceDeclaration> {
        self.descriptions().iter().find_map(|m| m.workspaces.get(code).cloned())
    }
    pub(crate) fn owner_of_purpose(&self, purpose: &str) -> Option<String> {
        self.descriptions().iter().find(|m| m.purposes.iter().any(|p| p == purpose)).map(|m| m.id.clone())
    }
    pub(crate) fn owner_of_action(&self, action: &str) -> Option<String> {
        self.descriptions().iter().find(|m| m.actions.iter().any(|p| p == action)).map(|m| m.id.clone())
    }
    pub(crate) fn local_owns(&self, owner: &str) -> bool { self.local.as_ref().is_some_and(|m| m.describe().id == owner) }
    pub(crate) fn local_purposes(&self) -> Vec<String> { self.local.as_ref().map(|m| m.describe().purposes).unwrap_or_default() }
    pub(crate) fn assets(&self) -> Vec<ModuleAsset> {
        self.descriptions().into_iter().map(|m| ModuleAsset {
            script: format!("/modules/{}/module.js?v={}", m.id, m.version),
            stylesheet: format!("/modules/{}/module.css?v={}", m.id, m.version),
            institution_codes: m.workspaces.keys().cloned().collect(),
            id: m.id, version: m.version, interface_version: m.interface_version, exports: m.frontend_exports,
        }).collect()
    }
    pub(crate) fn routes(&self) -> Vec<(String, RouteDescription)> {
        self.descriptions().into_iter().flat_map(|m| { let id = m.id; m.routes.into_iter().map(move |r| (id.clone(), r)) }).collect()
    }
    pub(crate) fn static_frontends(&self) -> Vec<(String, PathBuf)> {
        self.remotes.lock().map(|list| list.iter().filter(|m| m.available()).map(|m| (m.description.id.clone(), m.frontend.clone())).collect()).unwrap_or_default()
    }
    fn remote(&self, id: &str) -> Option<Arc<RemoteModule>> {
        self.remotes.lock().ok()?.iter().find(|m| m.description.id == id && m.available()).cloned()
    }
    pub(crate) async fn workspace_modules(&self, cid: &str, headers: &HeaderMap) -> Vec<String> {
        if let Some(local) = &self.local { return local.workspace_modules(cid).await; }
        let mut result = Vec::new();
        for id in self.descriptions().into_iter().filter(|m| m.instance_workspace).map(|m| m.id) {
            let Some(remote) = self.remote(&id) else { continue; };
            match remote.client.get(format!("{}/internal/workspace", remote.endpoint)).headers(headers.clone()).send().await {
                Ok(response) if response.status().is_success() => {
                    if let Ok(value) = response.json::<serde_json::Value>().await {
                        if let Some(items) = value.get("data").and_then(|v| v.as_array()) {
                            result.extend(items.iter().filter_map(|v| v.as_str().map(str::to_string)));
                        }
                    }
                }
                Ok(_) => tracing::warn!(module = %id, "module workspace unavailable"),
                Err(error) => tracing::warn!(module = %id, error = %error, "module workspace unavailable"),
            }
        }
        result
    }
    pub(crate) async fn load_installed(&self, database_url: &str, reserved: &[RouteDescription]) -> Result<(), String> {
        let directory = module_directory()?;
        if !directory.exists() { return Ok(()); }
        let mut occupied = route_keys(reserved)?;
        let mut purposes: BTreeSet<String> = crate::transactions::handler::builtin_purposes().into_iter().map(str::to_string).collect();
        let mut actions = BTreeSet::from([crate::authentication::operation_auth::AdminActionType::NodeBindingUnbind.as_str().to_string()]);
        let mut capabilities = BTreeSet::new();
        let mut entries = std::fs::read_dir(&directory).map_err(|e| e.to_string())?.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
        entries.sort_by_key(|v| v.file_name());
        for entry in entries {
            if let Err(error) = self.start_one(entry.path(), database_url, &mut occupied, &mut purposes, &mut actions, &mut capabilities).await {
                tracing::error!(module = %entry.file_name().to_string_lossy(), error = %error, "module not loaded");
            }
        }
        Ok(())
    }
    async fn start_one(&self, directory: PathBuf, database_url: &str, occupied: &mut BTreeSet<(String, String)>, purposes: &mut BTreeSet<String>, actions: &mut BTreeSet<String>, capabilities: &mut BTreeSet<String>) -> Result<(), String> {
        let metadata = std::fs::symlink_metadata(&directory).map_err(|e| e.to_string())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() { return Err("module must be an actual directory".into()); }
        let id = directory.file_name().and_then(|v| v.to_str()).ok_or("invalid module directory")?;
        if !valid_id(id) { return Err("invalid module id".into()); }
        let executable = directory.join(format!("onchina-{id}{}", std::env::consts::EXE_SUFFIX));
        let metadata = std::fs::symlink_metadata(&executable).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() { return Err("module executable must be a regular file".into()); }
        let description = read_description(&executable)?;
        validate_description(&description)?;
        if description.id != id { return Err("module directory and description mismatch".into()); }
        let keys = route_keys(&description.routes)?;
        if !occupied.is_disjoint(&keys) { return Err("module route conflict".into()); }
        if description.purposes.iter().any(|v| purposes.contains(v)) || description.actions.iter().any(|v| actions.contains(v)) || description.capabilities.keys().any(|v| capabilities.contains(v)) { return Err("module ownership conflict".into()); }
        let frontend = directory.join("frontend");
        verify_frontend(&frontend, &description)?;
        let socket = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
        let address = socket.local_addr().map_err(|e| e.to_string())?; drop(socket);
        let ca = crate::core::tls::organization_ca_certificate_pem()?;
        let client = reqwest::Client::builder().no_proxy().add_root_certificate(reqwest::Certificate::from_pem(ca.as_bytes()).map_err(|e| e.to_string())?)
            .resolve("onchina.local", address).connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(1300)).build().map_err(|e| e.to_string())?;
        let mut child = Command::new(&executable).env("DATABASE_URL", database_url).env("ONCHINA_EMBEDDED_PG", "0")
            .env("ONCHINA_MODULE_BIND", address.to_string()).env("ONCHINA_TLS_DIR", crate::core::tls::tls_dir())
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::inherit()).spawn().map_err(|e| e.to_string())?;
        let endpoint = format!("https://onchina.local:{}", address.port());
        let start = Instant::now();
        let ready = loop {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() || start.elapsed() > Duration::from_secs(120) { break false; }
            if let Ok(response) = client.get(format!("{endpoint}/api/health")).timeout(Duration::from_secs(2)).send().await {
                if let Ok(value) = response.json::<serde_json::Value>().await {
                    if value.pointer("/data/status").and_then(|v| v.as_str()) == Some("UP")
                        && value.pointer("/data/module_id").and_then(|v| v.as_str()) == Some(id)
                        && value.pointer("/data/module_version").and_then(|v| v.as_str()) == Some(description.version.as_str()) { break true; }
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        if !ready { let _ = child.kill(); let _ = child.wait(); return Err("module did not become ready".into()); }
        occupied.extend(keys); purposes.extend(description.purposes.iter().cloned()); actions.extend(description.actions.iter().cloned()); capabilities.extend(description.capabilities.keys().cloned());
        self.remotes.lock().map_err(|_| "module catalog lock poisoned")?.push(Arc::new(RemoteModule { description, client, endpoint, frontend, child: Mutex::new(child) }));
        Ok(())
    }
    pub(crate) fn stop_all(&self) {
        if let Ok(remotes) = self.remotes.lock() {
            for remote in remotes.iter() {
                if let Ok(mut child) = remote.child.lock() {
                    drop(child.stdin.take());
                    let start = Instant::now();
                    while matches!(child.try_wait(), Ok(None)) && start.elapsed() < Duration::from_secs(12) { std::thread::sleep(Duration::from_millis(25)); }
                    if matches!(child.try_wait(), Ok(None)) { let _ = child.kill(); }
                    let _ = child.wait();
                }
            }
        }
    }
}
pub(crate) fn module_directory() -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("ONCHINA_MODULE_DIR") {
        let path = PathBuf::from(value);
        if !path.is_absolute() { return Err("ONCHINA_MODULE_DIR must be absolute".into()); } return Ok(path);
    }
    if cfg!(target_os = "macos") {
        return Ok(PathBuf::from(std::env::var_os("HOME").ok_or("HOME unavailable")?).join("Library/Application Support/gmb.dev/onchina-modules"));
    }
    Ok(std::env::current_exe().map_err(|e| e.to_string())?.parent().ok_or("executable directory missing")?.join("modules"))
}
pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|v| v.is_ascii_lowercase() || v.is_ascii_digit() || v == b'_') && id.as_bytes()[0].is_ascii_lowercase()
}
pub(crate) fn validate_description(value: &ModuleDescription) -> Result<(), String> {
    if !valid_id(&value.id) || value.interface_version != model::MODULE_INTERFACE_VERSION || value.version.is_empty() || !value.version.bytes().all(|v| v.is_ascii_digit() || v == b'.') { return Err("unsupported module identity/version/interface".into()); }
    let source = Path::new(&value.frontend_source);
    if source.is_absolute() || source.components().any(|v| !matches!(v, std::path::Component::Normal(_))) || !value.frontend_source.starts_with("frontend/") { return Err("frontend source must stay inside its module".into()); }
    if value.frontend_exports.is_empty() || value.frontend_exports.iter().any(|v| v.is_empty() || !v.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')) { return Err("invalid frontend exports".into()); }
    route_keys(&value.routes)?;
    for values in [&value.purposes, &value.actions] {
        if values.iter().collect::<BTreeSet<_>>().len() != values.len() || values.iter().any(|v| v.is_empty()) { return Err("duplicate module ownership".into()); }
    }
    Ok(())
}
fn route_keys(routes: &[RouteDescription]) -> Result<BTreeSet<(String, String)>, String> {
    let mut keys = BTreeSet::new();
    for route in routes {
        if !matches!(route.group.as_str(), "auth" | "admin" | "public" | "app") || !route.path.starts_with("/api/") || route.methods.is_empty() || route.body_limit == 0 || route.body_limit > crate::documents::MAX_REQUEST_BYTES { return Err("invalid module route declaration".into()); }
        for method in &route.methods {
            if !matches!(method.as_str(), "GET" | "POST" | "PATCH" | "PUT" | "DELETE" | "HEAD" | "OPTIONS") || !keys.insert((method.clone(), route.path.clone())) { return Err("duplicate or unsupported route method".into()); }
        }
    } Ok(keys)
}
fn read_description(path: &Path) -> Result<ModuleDescription, String> {
    let mut child = Command::new(path).arg("--describe").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("module description pipe missing")?;
    let reader = std::thread::spawn(move || { let mut bytes = Vec::new(); stdout.take(1024 * 1024 + 1).read_to_end(&mut bytes).map(|_| bytes).map_err(|e| e.to_string()) });
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? { break status; }
        if start.elapsed() > Duration::from_secs(10) { let _ = child.kill(); let _ = child.wait(); let _ = reader.join(); return Err("module description timed out".into()); }
        std::thread::sleep(Duration::from_millis(10));
    };
    let bytes = reader.join().map_err(|_| "module description reader failed")??;
    if !status.success() || bytes.len() > 1024 * 1024 { return Err("module description failed or exceeded limit".into()); }
    serde_json::from_slice(&bytes).map_err(|e| format!("invalid module description: {e}"))
}
fn verify_frontend(directory: &Path, module: &ModuleDescription) -> Result<(), String> {
    let script = directory.join("module.js");
    let metadata = std::fs::symlink_metadata(&script).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() { return Err("module frontend must be a regular file".into()); }
    let text = std::fs::read_to_string(script).map_err(|e| e.to_string())?;
    let line = text.lines().next().ok_or("module frontend header missing")?;
    let json = line.strip_prefix("/*onchina:").and_then(|v| v.strip_suffix("*/")).ok_or("module frontend header invalid")?;
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if value["id"] != module.id || value["version"] != module.version || value["interface_version"] != module.interface_version { return Err("module backend/frontend versions mismatch".into()); }
    Ok(())
}
pub(crate) async fn forward_request(state: &AppState, owner: &str, request: Request, limit: usize) -> Response {
    let Some(remote) = state.modules.remote(owner) else { return api_error(StatusCode::SERVICE_UNAVAILABLE, 1503, "module unavailable"); };
    let (parts, body) = request.into_parts();
    let bytes = match axum::body::to_bytes(body, limit).await { Ok(bytes) => bytes, Err(_) => return api_error(StatusCode::PAYLOAD_TOO_LARGE, 1001, "request body too large") };
    let target = format!("{}{}", remote.endpoint, parts.uri.path_and_query().map(|v| v.as_str()).unwrap_or("/"));
    let mut headers = parts.headers;
    for name in ["host", "connection", "transfer-encoding", "content-length", "upgrade", "proxy-authorization", "proxy-authenticate", "te", "trailer"] { headers.remove(name); }
    match remote.client.request(parts.method, target).headers(headers).body(bytes).send().await {
        Ok(response) => {
            let status = response.status(); let mut headers = response.headers().clone();
            for name in ["connection", "transfer-encoding", "content-length", "upgrade", "te", "trailer"] { headers.remove(name); }
            match response.bytes().await {
                Ok(bytes) => (status, headers, Body::from(bytes)).into_response(),
                Err(error) => { tracing::warn!(module = %owner, error = %error, "module response failed"); api_error(StatusCode::BAD_GATEWAY, 1502, "module response unavailable") }
            }
        }
        Err(error) => { tracing::warn!(module = %owner, error = %error, "module request failed"); api_error(StatusCode::BAD_GATEWAY, 1502, "module unavailable") }
    }
}
pub(crate) async fn forward_json(state: &AppState, owner: &str, path: &str, headers: &HeaderMap, value: &impl Serialize) -> Response {
    let bytes = match serde_json::to_vec(value) { Ok(bytes) => bytes, Err(_) => return api_error(StatusCode::BAD_REQUEST, 1001, "invalid request") };
    let mut request = Request::new(Body::from(bytes)); *request.method_mut() = Method::POST;
    let Ok(uri) = path.parse() else { return api_error(StatusCode::BAD_REQUEST, 1001, "invalid route"); };
    *request.uri_mut() = uri; *request.headers_mut() = headers.clone();
    request.headers_mut().insert("content-type", axum::http::HeaderValue::from_static("application/json"));
    forward_request(state, owner, request, 2 * 1024 * 1024).await
}
pub(crate) async fn internal_workspace(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let ctx = match crate::require_admin_any(&state, &headers) { Ok(ctx) => ctx, Err(response) => return response };
    let Some(local) = &state.modules.local else { return api_error(StatusCode::NOT_FOUND, 1004, "module endpoint unavailable"); };
    Json(crate::ApiResponse { code: 0, message: "ok".into(), data: local.workspace_modules(&ctx.institution_cid_number).await }).into_response()
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
pub(crate) fn run_module_command(id: &str, command: &str, database_url: &str) -> Result<(), String> {
    if !valid_id(id) { return Err("invalid module id".into()); }
    let executable = module_directory()?.join(id).join(format!("onchina-{id}{}", std::env::consts::EXE_SUFFIX));
    let description = read_description(&executable)?;
    validate_description(&description)?;
    if description.id != id { return Err("module identity mismatch".into()); }
    let status = Command::new(executable).arg(command).env("DATABASE_URL", database_url)
        .env("ONCHINA_EMBEDDED_PG", "0").status().map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err("module maintenance failed".into()) }
}
#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn route_conflicts_include_method_and_path() {
        let route = RouteDescription { group: "admin".into(), path: "/api/example/{id}".into(), methods: vec!["GET".into()], body_limit: 2 * 1024 * 1024 };
        assert!(route_keys(&[route.clone()]).is_ok()); assert!(route_keys(&[route.clone(), route]).is_err());
    }
    #[test]
    fn identifiers_reject_paths() {
        for id in ["registry", "legislation", "private", "judicial"] { assert!(valid_id(id)); }
        for id in ["", "../registry", "/tmp/module", "Registry", "a-b"] { assert!(!valid_id(id)); }
    }
}

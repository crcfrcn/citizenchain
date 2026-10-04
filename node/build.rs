use serde_json::Value;
use std::{env, fs, path::Path};
use substrate_build_script_utils::{generate_cargo_keys, rerun_if_git_head_changed};

fn main() {
    generate_cargo_keys();
    rerun_if_git_head_changed();

    build_tauri().expect("Tauri 构建失败");
}

// tauri-build 固定向当前目录写 gen/schemas；只在 Cargo 的中央 OUT_DIR 中运行它。
// Rust 源码和 generate_context! 仍读取原始工程，权限直接来自 tauri.conf.json。
fn build_tauri() -> Result<(), Box<dyn std::error::Error>> {
    let source = env::current_dir()?;
    let work = std::path::PathBuf::from(env::var_os("OUT_DIR").ok_or("缺少 OUT_DIR")?)
        .join("tauri");
    let repository = source.parent().and_then(Path::parent).ok_or("缺少仓库根目录")?;
    if !work.is_absolute() || work.starts_with(repository) {
        return Err("Tauri 生成文件不能写入公民链源码目录".into());
    }
    let target = tauri_utils::platform::Target::from_triple(&env::var("TARGET")?);
    let (mut config, paths) = tauri_utils::config::parse::read_from(target, &source)?;
    for path in paths {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    println!(
        "cargo:rerun-if-changed={}",
        source.join("Cargo.toml").display()
    );
    let original_override = env::var_os("TAURI_CONFIG");
    if let Some(value) = &original_override {
        json_patch::merge(
            &mut config,
            &serde_json::from_str(&value.to_string_lossy())?,
        );
    }
    // 保持源码配置和 CLI 覆盖中的资源引用含义，不受临时工作目录影响。
    if let Some(resources) = config.pointer_mut("/bundle/resources") {
        match resources {
            Value::Array(paths) => paths.iter_mut().for_each(|path| absolute_path(path, &source)),
            Value::Object(paths) => {
                *paths = std::mem::take(paths)
                    .into_iter()
                    .map(|(path, destination)| {
                        (source.join(path).to_string_lossy().into_owned(), destination)
                    })
                    .collect();
            }
            _ => {}
        }
    }
    for key in ["/bundle/icon", "/bundle/externalBin"] {
        if let Some(Value::Array(paths)) = config.pointer_mut(key) {
            paths.iter_mut().for_each(|path| absolute_path(path, &source));
        }
    }
    if let Some(path) = config.pointer_mut("/bundle/windows/webviewInstallMode/path") {
        absolute_path(path, &source);
    }
    fs::create_dir_all(&work)?;
    // Cargo.toml 的 workspace 继承继续指向原始工作空间；不复制或编译另一套源码。
    let workspace = serde_json::to_string(&source.parent().ok_or("缺少工作空间")?)?;
    let manifest = fs::read_to_string(source.join("Cargo.toml"))?
        .replacen("[package]", &format!("[package]\nworkspace = {workspace}"), 1);
    write_if_changed(&work.join("Cargo.toml"), manifest.as_bytes())?;
    let config = serde_json::to_string(&config)?;
    write_if_changed(&work.join("tauri.conf.json"), config.as_bytes())?;
    env::set_current_dir(&work)?;
    env::set_var("TAURI_CONFIG", &config);
    let result = tauri_build::try_build(tauri_build::Attributes::new());
    env::set_current_dir(&source)?;
    match original_override {
        Some(value) => env::set_var("TAURI_CONFIG", value),
        None => env::remove_var("TAURI_CONFIG"),
    }
    result.map_err(Into::into)
}

fn absolute_path(value: &mut Value, source: &Path) {
    if let Some(path) = value.as_str() {
        *value = Value::String(source.join(path).to_string_lossy().into_owned());
    }
}

fn write_if_changed(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if fs::read(path).ok().as_deref() != Some(contents) {
        fs::write(path, contents)?;
    }
    Ok(())
}

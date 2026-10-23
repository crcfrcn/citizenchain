use serde_json::Value;
use std::{env, fs, path::Path};
use substrate_build_script_utils::{generate_cargo_keys, rerun_if_git_head_changed};

// 只统一Windows盘符及UNC扩展长度前缀的表示，不解析链接、不折叠..或改写目录大小写。
fn comparison_path(path: &Path) -> std::path::PathBuf {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        use std::path::{Component, Prefix};
        let original: Vec<u16> = path.as_os_str().encode_wide().collect();
        let mut units = match path.components().next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::VerbatimDisk(_) => original[4..].to_vec(),
                Prefix::VerbatimUNC(_, _) => {
                    let mut unc = vec![92, 92];
                    unc.extend_from_slice(&original[8..]);
                    unc
                }
                Prefix::Disk(_) => original,
                _ => return path.to_path_buf(),
            },
            _ => return path.to_path_buf(),
        };
        if units.len() >= 2 && units[1] == u16::from(b':') && (97..=122).contains(&units[0]) {
            units[0] -= 32;
        }
        std::ffi::OsString::from_wide(&units).into()
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    comparison_path(left) == comparison_path(right)
}

fn path_within(path: &Path, root: &Path) -> bool {
    comparison_path(path).starts_with(comparison_path(root))
}

fn local_build_work(original: &Path, task: &Path, platform: &str) -> bool {
    platform == "macos"
        && task.parent().is_some_and(|parent| same_path(parent, &original.join("target/build")))
        && task.file_name().and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix("local-"))
            .is_some_and(|suffix| suffix.len() == 6 && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric()))
}

fn main() {
    generate_cargo_keys();
    rerun_if_git_head_changed();

    build_tauri().expect("Tauri 构建失败");
}

// tauri-build 固定向当前目录写 gen/schemas；只在本仓 target 内的规范 OUT_DIR 中运行它。
// Rust 源码和 generate_context! 仍读取原始工程，权限直接来自 tauri.conf.json。
fn build_tauri() -> Result<(), Box<dyn std::error::Error>> {
    let source = env::current_dir()?;
    let output = std::path::PathBuf::from(env::var_os("OUT_DIR").ok_or("缺少 OUT_DIR")?);
    let work = output.join("tauri");
    let repository = source.parent().ok_or("缺少仓库根目录")?.canonicalize()?;
    // 远端工程副本读取自身源码，编译输出仍只能属于原产品的对应平台现场。
    let platforms = ["macos", "windows", "linux-arm", "linux-amd", "wasm"];
    let target_root = match (
        env::var_os("CITIZENCHAIN_SOURCE_ROOT"),
        env::var_os("CITIZENCHAIN_WORK_DIR"),
    ) {
        (Some(original), Some(task)) => {
            let original = std::path::PathBuf::from(original).canonicalize()?;
            let task = std::path::PathBuf::from(task).canonicalize()?;
            if !platforms.iter().any(|platform| same_path(&task, &original.join("target/build").join(platform)))
                && !local_build_work(&original, &task, &env::var("CITIZENCHAIN_BUILD_PLATFORM").unwrap_or_default())
                && !same_path(&task, &original.join("target/test")) {
                return Err("Tauri 任务只能使用公民链固定工作根".into());
            }
            if !same_path(&repository, &original) && !same_path(&repository, &task.join("source")) {
                return Err("Tauri 源码不属于公民链原件或本轮工程副本".into());
            }
            task
        }
        (None, None) => {
            let build = repository.join("target/build");
            let test = repository.join("target/test");
            if let Some(platform_work) = platforms.iter().map(|platform| build.join(platform)).find(|work| path_within(&output, work)) {
                platform_work
            } else if path_within(&output, &test) {
                test
            } else {
                return Err("Tauri 输出只能位于公民链固定 build 或 test 目录".into());
            }
        }
        _ => return Err("Tauri 原产品与固定任务身份必须同时提供".into()),
    };
    if !output.is_absolute()
        || !path_within(&output, &target_root)
        || !same_path(&output.canonicalize()?, &output)
        || (fs::symlink_metadata(&work).is_ok() && !same_path(&work.canonicalize()?, &work))
    {
        return Err("Tauri 生成目录必须是公民链 target 内的规范路径".into());
    }
    build_rpc_assets(&source, &output)?;
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
            Value::Array(paths) => paths
                .iter_mut()
                .for_each(|path| absolute_path(path, &source)),
            Value::Object(paths) => {
                *paths = std::mem::take(paths)
                    .into_iter()
                    .map(|(path, destination)| {
                        (
                            source.join(path).to_string_lossy().into_owned(),
                            destination,
                        )
                    })
                    .collect();
            }
            _ => {}
        }
    }
    for key in ["/bundle/icon", "/bundle/externalBin"] {
        if let Some(Value::Array(paths)) = config.pointer_mut(key) {
            paths
                .iter_mut()
                .for_each(|path| absolute_path(path, &source));
        }
    }
    if let Some(path) = config.pointer_mut("/bundle/windows/webviewInstallMode/path") {
        absolute_path(path, &source);
    }
    fs::create_dir_all(&work)?;
    // Cargo.toml 的 workspace 继承继续指向原始工作空间；不复制或编译另一套源码。
    let workspace = serde_json::to_string(&source.parent().ok_or("缺少工作空间")?)?;
    let manifest = fs::read_to_string(source.join("Cargo.toml"))?.replacen(
        "[package]",
        &format!("[package]\nworkspace = {workspace}"),
        1,
    );
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

// 接入页只从唯一 JS 真源生成到已核对的 OUT_DIR，正式节点不依赖外置静态目录。
fn build_rpc_assets(source: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let generator = source.join("frontend/metamask.mjs");
    let icon = source.join("../icons/gmb.png");
    println!("cargo:rerun-if-changed={}", generator.display());
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-env-changed=NODE");
    let node = std::path::PathBuf::from(env::var_os("NODE").ok_or("接入页构建缺少已交付的 NODE")?);
    if !node.is_absolute()
        || !fs::symlink_metadata(&node)?.is_file()
        || !same_path(&node.canonicalize()?, &node)
    {
        return Err("接入页生成必须使用已交付的普通 Node 执行器绝对路径".into());
    }
    let result = std::process::Command::new(node)
        .arg(&generator)
        .env_remove("NODE_TEST_CONTEXT")
        .env_remove("NODE_OPTIONS")
        .env_remove("NODE_PATH")
        .env_remove("CITIZENCHAIN_TEST_PAGE_OUTPUT")
        .output()?;
    // 错误只报告固定原因，生成器的环境和诊断不进入构建日志。
    if !result.status.success() || !result.stderr.is_empty() {
        return Err("唯一接入页生成入口执行失败".into());
    }
    let page = std::str::from_utf8(&result.stdout)?;
    if page.len() > 128 * 1024
        || !page.starts_with("<!doctype html>")
        || !page.ends_with("</html>\n")
    {
        return Err("接入页生成结果不是有界的完整 HTML".into());
    }
    write_if_changed(&output.join("metamask.html"), page.as_bytes())?;
    Ok(())
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

#[cfg(test)]
mod path_tests {
    use super::{comparison_path, path_within, same_path, local_build_work};
    use std::path::{Path, PathBuf};

    #[test]
    fn comparison_keeps_directory_boundaries_and_different_targets() {
        let root = PathBuf::from(if cfg!(windows) { r"C:\repo\target\build\windows" } else { "/repo/target/build/windows" });
        assert!(path_within(&root.join("work/out"), &root));
        assert!(!path_within(&root.with_file_name("windows-other").join("out"), &root));
        assert!(!same_path(&root.join("work/../out"), &root.join("out")));
        assert!(!same_path(&root.join("link/out"), &root.join("real/out")));
    }

    #[test]
    fn local_macos_work_is_limited_to_the_compiler_generated_directory() {
        let root = PathBuf::from(if cfg!(windows) { r"C:\repo" } else { "/repo" });
        let build = root.join("target/build");
        assert!(local_build_work(&root, &build.join("local-Abc123"), "macos"));
        assert!(!local_build_work(&root, &build.join("local-Abc123"), "windows"));
        assert!(!local_build_work(&root, &build.join("local-Abc123/nested"), "macos"));
        assert!(!local_build_work(&root, &root.join("target/build-other/local-Abc123"), "macos"));
        for name in ["local-", "local-short", "local-toolong", "local-中文汉字", "other-Abc123"] {
            assert!(!local_build_work(&root, &build.join(name), "macos"));
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_disk_and_unc_prefixes_compare_without_relaxing_ownership() {
        assert!(same_path(Path::new(r"C:\repo\target\out"), Path::new(r"\\?\C:\repo\target\out")));
        assert!(same_path(Path::new(r"c:\repo\target\out"), Path::new(r"\\?\C:\repo\target\out")));
        assert!(same_path(Path::new(r"\\server\share\repo\out"), Path::new(r"\\?\UNC\server\share\repo\out")));
        assert!(path_within(Path::new(r"\\?\C:\repo\target\out"), Path::new(r"C:\repo\target")));
        assert!(!same_path(Path::new(r"D:\repo\out"), Path::new(r"\\?\C:\repo\out")));
        assert!(!same_path(Path::new(r"C:\repo\out"), Path::new(r"\\.\C:\repo\out")));
        assert!(!path_within(Path::new(r"\\?\C:\repo\target-other\out"), Path::new(r"C:\repo\target")));
        assert!(!same_path(Path::new(r"C:\repo\link\out"), Path::new(r"\\?\C:\repo\real\out")));
        assert!(!Path::new(r"C:repo\out").is_absolute());
    }

    #[cfg(windows)]
    #[test]
    fn windows_comparison_preserves_native_path_code_units() {
        use std::os::windows::ffi::OsStringExt;
        let raw = [67, 58, 92, 0xd800];
        let mut extended = vec![92, 92, 63, 92];
        extended.extend_from_slice(&raw);
        let plain = PathBuf::from(std::ffi::OsString::from_wide(&raw));
        let extended = PathBuf::from(std::ffi::OsString::from_wide(&extended));
        let other = PathBuf::from(std::ffi::OsString::from_wide(&[67, 58, 92, 0xd801]));
        assert!(same_path(&plain, &extended));
        assert!(!same_path(&plain, &other));
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_paths_keep_the_original_representation() {
        let path = Path::new(r"\\?\C:\repo\out");
        assert_eq!(comparison_path(path), path);
        assert!(!same_path(path, Path::new(r"C:\repo\out")));
    }
}

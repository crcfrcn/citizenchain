// 仓库守卫用 Result 传播读取与结构错误；合同不符仍由原有断言使测试失败。
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repository_root() -> Result<PathBuf, Box<dyn Error>> {
    Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("qr-protocol必须位于citizenchain/crates/protocol")?
        .to_path_buf())
}

// 检查版本控制内的源码；忽略的本机生成物不属于产品源码合同，Git失败必须阻断。
fn collect_files(root: &Path, output: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    let repository = repository_root()?;
    let scope = root.strip_prefix(&repository)?;
    let listed = Command::new("git")
        .arg("-C")
        .arg(&repository)
        .args(["ls-files", "--cached", "--full-name", "-z", "--"])
        .arg(if scope.as_os_str().is_empty() {
            Path::new(".")
        } else {
            scope
        })
        .output()?;
    if !listed.status.success() {
        return Err("读取Git跟踪源码清单失败".into());
    }
    let paths = String::from_utf8(listed.stdout)?;
    for relative in paths.split('\0').filter(|path| !path.is_empty()) {
        let path = repository.join(relative);
        if !path.starts_with(root) || !path.is_file() {
            return Err(format!("Git跟踪源码缺失或路径越界：{}", path.display()).into());
        }
        output.push(path);
    }
    Ok(())
}

fn source_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some(
            "c" | "cc"
                | "cpp"
                | "dart"
                | "h"
                | "hpp"
                | "java"
                | "js"
                | "json"
                | "kts"
                | "md"
                | "mjs"
                | "proto"
                | "py"
                | "rs"
                | "sh"
                | "sql"
                | "swift"
                | "toml"
                | "ts"
                | "tsx"
                | "yaml"
                | "yml"
        )
    )
}

// 扫描范围不能扩展到仓库外；拒绝必须发生在调用Git或读取其它目录之前。
#[test]
fn tracked_source_scope_rejects_repository_parent() -> Result<(), Box<dyn Error>> {
    let root = repository_root()?;
    let mut files = Vec::new();
    assert!(collect_files(root.parent().ok_or("仓库父目录缺失")?, &mut files).is_err());
    assert!(files.is_empty());
    Ok(())
}

// 按实际YAML映射验证职责与依赖，不依赖Job的文本排列顺序。
fn validate_workflow(document: &serde_yaml::Value, platform: &str) -> Result<(), Box<dyn Error>> {
    let jobs = document["jobs"].as_mapping().ok_or("Workflow缺少jobs映射")?;
    let names = ["prepare", "build", "release", "request-finalize", "finalize"];
    if jobs.len() != names.len() || names.iter().any(|name| !document["jobs"][*name].is_mapping())
        || !document["on"]["workflow_dispatch"].is_mapping()
        || !document["on"]["repository_dispatch"].is_mapping()
        || document["jobs"]["build"]["needs"].as_str() != Some("prepare")
        || document["jobs"]["release"]["needs"].as_str() != Some("build")
    {
        return Err("Workflow阶段或编译发布依赖不符".into());
    }
    let command = format!("node scripts/build.mjs {platform}");
    let mut compiler_calls = 0;
    for name in names {
        let steps = document["jobs"][name]["steps"].as_sequence().ok_or("Job缺少步骤")?;
        for step in steps {
            let run = step["run"].as_str().unwrap_or("");
            let script = step["with"]["script"].as_str().unwrap_or("");
            if run.contains("scripts/build.mjs") || script.contains("scripts/build.mjs") {
                if name != "build" || !run.lines().any(|line| line.trim() == command) {
                    return Err("只有build Job可调用本平台编译入口".into());
                }
                compiler_calls += 1;
            }
        }
    }
    if compiler_calls != 1
        || !document["jobs"]["finalize"]["if"].as_str().unwrap_or("").contains("repository_dispatch")
    {
        return Err("编译入口必须唯一，收尾必须只接收所属事件".into());
    }
    Ok(())
}

#[test]
fn github_platform_workflows_keep_product_owned_stages() -> Result<(), Box<dyn Error>> {
    let root = repository_root()?;
    let workflow_root = root.join(".github/workflows");
    let entries = fs::read_dir(&workflow_root)
        .map_err(|error| format!("读取CitizenChain Workflow目录失败：{error}"))?
        .flatten()
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 5);
    let forbidden = [
        ["TATA", "_CONSOLE"].concat(),
        ["Tata", "Console"].concat(),
        ["tata", "console"].concat(),
        [".tata", "-flow"].concat(),
        [".publish", "'"].concat(),
    ];
    for entry in entries {
        assert!(entry.ends_with(".yml") && entry != "repository.yml");
        let workflow = fs::read_to_string(workflow_root.join(&entry))
            .map_err(|error| format!("读取CitizenChain Workflow失败：{error}"))?;
        for value in &forbidden {
            assert!(
                !workflow.contains(value.as_str()),
                "Workflow包含禁止边界：{value}"
            );
        }
        let platform = match entry.as_str() {
            "macos.yml" => "macos",
            "windows.yml" => "windows",
            "linuxarm.yml" => "linux-arm",
            "linuxamd.yml" => "linux-amd",
            "wasm.yml" => "wasm",
            _ => return Err("出现未声明平台Workflow".into()),
        };
        let document: serde_yaml::Value = serde_yaml::from_str(&workflow)?;
        validate_workflow(&document, platform)?;
        // 缺阶段、发布绕过编译、收尾误编译都必须继续被守卫拒绝。
        let mut missing = document.clone();
        missing["jobs"].as_mapping_mut().ok_or("缺少jobs映射")?
            .remove(&serde_yaml::Value::String("build".into()));
        assert!(validate_workflow(&missing, platform).is_err());
        let mut bypass = document.clone();
        bypass["jobs"]["release"]["needs"] = serde_yaml::Value::String("prepare".into());
        assert!(validate_workflow(&bypass, platform).is_err());
        let mut recompile = document.clone();
        recompile["jobs"]["finalize"]["steps"][0]["run"] =
            serde_yaml::Value::String(format!("node scripts/build.mjs {platform}"));
        assert!(validate_workflow(&recompile, platform).is_err());
    }
    Ok(())
}

#[test]
fn product_sources_do_not_depend_on_the_control_program() -> Result<(), Box<dyn Error>> {
    let root = repository_root()?;
    let protected_runtime = root.join("runtime");
    let forbidden = [
        ["TATA", "_CONSOLE"].concat(),
        ["Tata", "Console"].concat(),
        ["tata", "console"].concat(),
        [".tata", "-flow"].concat(),
    ];
    let mut files = Vec::new();
    collect_files(&root, &mut files)?;
    let mut violations = Vec::new();
    for path in files {
        // 技术文档可以描述控制台如何调用产品；依赖检查只针对可执行源码。
        if path.starts_with(&protected_runtime)
            || path == root.join("crates/protocol/tests/repo_guard.rs")
            || path.extension().and_then(|value| value.to_str()) == Some("md")
            || !source_file(&path)
        {
            continue;
        }
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("读取跟踪源码失败 {}：{error}", path.display()))?;
        for value in &forbidden {
            if source.contains(value) {
                violations.push(format!("{}: {value}", path.display()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "产品源码仍依赖控制程序：\n{}",
        violations.join("\n")
    );
    Ok(())
}

#[test]
fn product_flow_directories_have_one_final_name() -> Result<(), Box<dyn Error>> {
    let root = repository_root()?;
    assert!(root.join("scripts").is_dir(), "CitizenChain缺少scripts目录");
    for forbidden in ["pipeline", "pipeline-support", "packaging"] {
        assert!(
            !root.join(forbidden).exists(),
            "仓库根保留禁止流程目录：{forbidden}"
        );
    }
    Ok(())
}

#[test]
fn runtime_upgrade_implementation_is_not_in_product() -> Result<(), Box<dyn Error>> {
    let root = repository_root()?;
    let mut files = Vec::new();
    collect_files(&root, &mut files)?;
    for name in [
        "build_request.mjs",
        "fetch_wasm.mjs",
        "submit_request.mjs",
        "tx_common.mjs",
    ] {
        assert!(
            files
                .iter()
                .all(|path| path.file_name().and_then(|value| value.to_str()) != Some(name)),
            "CitizenChain产品目录禁止保存Runtime开发升级实现：{name}"
        );
    }
    Ok(())
}

#[test]
fn only_qr_v1_is_versioned() -> Result<(), Box<dyn Error>> {
    let root = repository_root()?;
    let mut files = Vec::new();
    collect_files(&root, &mut files)?;
    let prefix = ["QR", "_V"].concat();
    let mut violations = Vec::new();
    for path in files {
        if !source_file(&path) {
            continue;
        }
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("读取跟踪源码失败 {}：{error}", path.display()))?;
        for token in
            source.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        {
            if token.starts_with(&prefix) && token != "QR_V1" {
                violations.push(format!("{}: {token}", path.display()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "发现禁止协议版本：\n{}",
        violations.join("\n")
    );
    Ok(())
}

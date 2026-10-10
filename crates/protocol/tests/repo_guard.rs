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

#[test]
fn github_entry_only_runs_product_ci_and_release() -> Result<(), Box<dyn Error>> {
    let root = repository_root()?;
    let workflow_root = root.join(".github/workflows");
    let entries = fs::read_dir(&workflow_root)
        .map_err(|error| format!("读取CitizenChain Workflow目录失败：{error}"))?
        .flatten()
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 11);
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
        if entry == "tatagate.yml" {
            assert!(workflow.contains("  gate:"));
            assert!(workflow.contains("  push:"));
            assert!(workflow.contains("branches: [main]"));
            assert!(!workflow.contains("workflow_dispatch:"));
        } else {
            assert!(workflow.contains("  flow:"));
            assert!(workflow.contains("workflow_dispatch:"));
        }
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
        if path.starts_with(&protected_runtime)
            || path == root.join(".github/tatagate/tatagate.mjs")
            || path == root.join("crates/protocol/tests/repo_guard.rs")
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
fn citizenchain_owns_one_exact_protoc_dependency_path() -> Result<(), Box<dyn Error>> {
    let root = repository_root()?;
    let scripts = root.join("scripts");
    let contract: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(scripts.join("dependencies.json"))
            .map_err(|error| format!("读取CitizenChain依赖声明失败：{error}"))?,
    )
    .map_err(|error| format!("CitizenChain依赖声明不是有效JSON：{error}"))?;
    assert_eq!(contract["schema"], 1);
    assert_eq!(contract["tools"]["protoc"]["version"], "35.0");
    assert_eq!(
        contract["tools"]["protoc"]["source"],
        "https://github.com/protocolbuffers/protobuf/releases/tag/v35.0"
    );
    let archives = contract["tools"]["protoc"]["archives"]
        .as_object()
        .ok_or("CitizenChain protoc缺少四端归档")?;
    assert_eq!(archives.len(), 4);
    for platform in ["macos", "windows", "linux-arm", "linux-amd"] {
        let archive = archives
            .get(platform)
            .ok_or("CitizenChain protoc缺少平台归档")?;
        assert!(archive["url"]
            .as_str()
            .is_some_and(|value| value.starts_with(
                "https://github.com/protocolbuffers/protobuf/releases/download/v35.0/protoc-35.0-"
            )));
        assert!(archive["sha256"].as_str().is_some_and(
            |value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        ));
    }

    let dependency_source = fs::read_to_string(scripts.join("dependencies.mjs"))
        .map_err(|error| format!("读取CitizenChain依赖准备器失败：{error}"))?;
    assert!(dependency_source.contains("libprotoc ${expectedVersion}"));
    let local_source = fs::read_to_string(scripts.join("prepare-toolchain.sh"))
        .map_err(|error| format!("读取CitizenChain本机工具准备器失败：{error}"))?;
    assert!(local_source.contains("dependencies.mjs\" prepare protoc"));
    assert!(local_source.contains("export PROTOC"));

    let mut files = Vec::new();
    collect_files(&scripts.join("node"), &mut files)?;
    collect_files(&scripts.join("runtime"), &mut files)?;
    let mut prepared_jobs = 0;
    let mut violations = Vec::new();
    for path in files {
        if path.extension().and_then(|value| value.to_str()) != Some("mjs")
            || path.file_name().and_then(|value| value.to_str()) == Some("test.mjs")
        {
            continue;
        }
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("读取CitizenChain流程脚本失败：{error}"))?;
        for forbidden in [
            ["protobuf", "compiler"].join("-"),
            ["command", "-v", "protoc"].join(" "),
        ] {
            if source.contains(forbidden.as_str()) {
                violations.push(format!("{}: {forbidden}", path.display()));
            }
        }
        if source.contains("dependencies.mjs prepare protoc") {
            prepared_jobs += 1;
            assert!(source.contains("RUNNER_TEMP/citizenchain-protoc"));
            assert!(source.contains("PROTOC=%s") || source.contains("PROTOC=$protoc_executable"));
        }
    }
    assert_eq!(
        prepared_jobs, 14,
        "CitizenChain protoc必须接入十个CI Job和四个Release Job"
    );
    assert!(
        violations.is_empty(),
        "CitizenChain仍保留系统protoc：\n{}",
        violations.join("\n")
    );
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
        if !source_file(&path) || path == root.join(".github/tatagate/tatagate.mjs") {
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

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

// 四端只有一个原编译Run；最后Job不能再派发、重编、取消或吞掉清理失败。
fn validate_native_workflow(document: &serde_yaml::Value, platform: &str) -> Result<(), Box<dyn Error>> {
    let jobs = document["jobs"].as_mapping().ok_or("Workflow缺少jobs映射")?;
    let names = ["prepare", "build", "release", "cleanup"];
    let triggers = document["on"].as_mapping().ok_or("缺少触发声明")?;
    let cleanup = &document["jobs"]["cleanup"];
    let dependencies = cleanup["needs"].as_sequence().ok_or("清理缺少依赖")?;
    if jobs.len() != names.len() || names.iter().any(|name| !document["jobs"][*name].is_mapping())
        || jobs.keys().last().and_then(serde_yaml::Value::as_str) != Some("cleanup")
        || triggers.len() != 1 || !document["on"]["workflow_dispatch"].is_mapping()
        || document["jobs"]["build"]["needs"].as_str() != Some("prepare")
        || document["jobs"]["release"]["needs"].as_str() != Some("build")
        || dependencies.iter().map(serde_yaml::Value::as_str).collect::<Vec<_>>()
            != vec![Some("prepare"), Some("build"), Some("release")]
        || cleanup["if"].as_str() != Some("${{ !cancelled() && github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main' }}")
        || !(cleanup["continue-on-error"].is_null() || cleanup["continue-on-error"].as_bool() == Some(false))
        || cleanup["permissions"]["actions"].as_str() != Some("write")
        || cleanup["permissions"]["contents"].as_str() != Some("write")
    {
        return Err("四端必须在同一原Run中执行准备、编译、发布及末尾清理".into());
    }
    let command = format!("node scripts/build.mjs {platform}");
    let mut calls = 0;
    let mut cleanup_script = false;
    for name in names {
        let steps = document["jobs"][name]["steps"].as_sequence().ok_or("Job缺少步骤")?;
        for step in steps {
            let run = step["run"].as_str().unwrap_or("");
            let script = step["with"]["script"].as_str().unwrap_or("");
            if run.contains("scripts/build.mjs") || script.contains("scripts/build.mjs") {
                if name != "build" || !run.lines().any(|line| line.trim() == command) {
                    return Err("只有build Job可调用本平台编译入口".into());
                }
                calls += 1;
            }
            if name == "cleanup" {
                if !(step["continue-on-error"].is_null() || step["continue-on-error"].as_bool() == Some(false))
                    || ["createDispatchEvent", "createWorkflowDispatch", "dispatches", "/rerun", "reRunWorkflow", "reRunJob", "reRunFailedJobs", "/cancel", "cancelWorkflowRun", "forceCancelWorkflowRun"]
                        .iter().any(|value| run.contains(value) || script.contains(value))
                {
                    return Err("清理不能派发、重跑、取消或忽略失败".into());
                }
                cleanup_script |= script.contains("cfg.nativeCleanupPlan") && script.contains("deleteWorkflowRun");
            }
        }
    }
    if calls != 1 || !cleanup_script { return Err("编译或真实清理入口缺失".into()); }
    Ok(())
}

// 按实际YAML映射验证职责与依赖，不依赖Job的文本排列顺序。
fn validate_workflow(document: &serde_yaml::Value, platform: &str) -> Result<(), Box<dyn Error>> {
    if platform != "wasm" { return validate_native_workflow(document, platform); }
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
    let command = if platform == "wasm" {
        "node scripts/runtime.mjs build".to_string()
    } else {
        format!("node scripts/build.mjs {platform}")
    };
    let mut compiler_calls = 0;
    for name in names {
        let steps = document["jobs"][name]["steps"].as_sequence().ok_or("Job缺少步骤")?;
        for step in steps {
            let run = step["run"].as_str().unwrap_or("");
            let script = step["with"]["script"].as_str().unwrap_or("");
            if run.contains("scripts/build.mjs") || script.contains("scripts/build.mjs")
                || run.contains("scripts/runtime.mjs build")
                || script.contains("scripts/runtime.mjs build")
            {
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
    // WASM的显示号来自派发前读链加1；跳过的Job也必须使用所属事件的版本字段。
    let display = document["run-name"].as_str().ok_or("WASM缺少Run名称")?;
    if !display.starts_with("公民链·WASM·v")
        || !display.contains("inputs.version")
        || !display.contains("github.event.client_payload.version")
        || document["on"]["workflow_dispatch"]["inputs"]["transaction_version"]["required"].as_bool() != Some(true)
        || names.iter().any(|name| document["jobs"][*name]["name"].as_str() != Some(display))
        || document["jobs"]["release"]["env"]["RUNTIME_BUILD"].as_str() != Some("${{ needs.build.outputs.runtime }}")
        || document["jobs"]["release"]["env"]["BUILT_VERSION"].as_str() != Some("${{ needs.build.outputs.version }}")
    {
        return Err("WASM整数版本、双版本输入或GitHub名称未连通".into());
    }
    for name in names {
        for step in document["jobs"][name]["steps"].as_sequence().ok_or("WASM缺少步骤")? {
            let script = step["with"]["script"].as_str().unwrap_or("");
            if script.contains("nextDeliveryVersion") || script.contains("deliveryVersion(")
                || script.contains("node/tauri.conf.json")
            {
                return Err("WASM不能使用节点软件版本或SemVer推算".into());
            }
            let action = step["uses"].as_str().unwrap_or("");
            if (action.starts_with("actions/upload-artifact@") || action.starts_with("actions/download-artifact@"))
                && step["with"]["name"].as_str() != Some(display)
            {
                return Err("WASM Artifact名称与Runtime版本不一致".into());
            }
        }
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
    let mut native_cleanup: Option<String> = None;
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
        if platform != "wasm" {
            let steps = document["jobs"]["cleanup"]["steps"].as_sequence().ok_or("清理缺少步骤")?;
            let script = steps.last().and_then(|step| step["with"]["script"].as_str()).ok_or("清理缺少正文")?;
            if let Some(expected) = &native_cleanup { assert_eq!(script, expected, "四端清理正文必须相同"); }
            else { native_cleanup = Some(script.to_string()); }
            let mut detached = document.clone();
            detached["on"].as_mapping_mut().ok_or("缺少触发映射")?
                .insert(serde_yaml::Value::String("repository_dispatch".into()), serde_yaml::Value::Mapping(Default::default()));
            assert!(validate_workflow(&detached, platform).is_err());
            let mut ignored = document.clone();
            ignored["jobs"]["cleanup"]["continue-on-error"] = serde_yaml::Value::Bool(true);
            assert!(validate_workflow(&ignored, platform).is_err());
            let mut missing_need = document.clone();
            missing_need["jobs"]["cleanup"]["needs"] = serde_yaml::Value::Sequence(vec![serde_yaml::Value::String("build".into())]);
            assert!(validate_workflow(&missing_need, platform).is_err());
        }
        // 缺阶段、发布绕过编译、收尾误编译都必须继续被守卫拒绝。
        let mut missing = document.clone();
        missing["jobs"].as_mapping_mut().ok_or("缺少jobs映射")?
            .remove(&serde_yaml::Value::String("build".into()));
        assert!(validate_workflow(&missing, platform).is_err());
        let mut bypass = document.clone();
        bypass["jobs"]["release"]["needs"] = serde_yaml::Value::String("prepare".into());
        assert!(validate_workflow(&bypass, platform).is_err());
        let mut recompile = document.clone();
        let cleanup_name = if platform == "wasm" { "finalize" } else { "cleanup" };
        recompile["jobs"][cleanup_name]["steps"][0]["run"] =
            serde_yaml::Value::String(if platform == "wasm" {
                "node scripts/runtime.mjs build".to_string()
            } else {
                format!("node scripts/build.mjs {platform}")
            });
        assert!(validate_workflow(&recompile, platform).is_err());
        if platform == "wasm" {
            let mut old_entry = document.clone();
            let steps = old_entry["jobs"]["build"]["steps"].as_sequence_mut().ok_or("缺少build步骤")?;
            let compile = steps.iter_mut().find(|step| {
                step["run"].as_str().unwrap_or("").contains("node scripts/runtime.mjs build")
            }).ok_or("缺少Runtime编译步骤")?;
            compile["run"] = serde_yaml::Value::String("node scripts/build.mjs wasm".into());
            assert!(validate_workflow(&old_entry, platform).is_err());
            // 仅WASM增加负例，四端既有阶段及夹具保持。
            let mut missing_transaction = document.clone();
            missing_transaction["on"]["workflow_dispatch"]["inputs"].as_mapping_mut().ok_or("缺少inputs")?
                .remove(&serde_yaml::Value::String("transaction_version".into()));
            assert!(validate_workflow(&missing_transaction, platform).is_err());
            let mut wrong_name = document.clone();
            wrong_name["jobs"]["finalize"]["name"] = serde_yaml::Value::String("公民链·WASM·v${{ inputs.version }}".into());
            assert!(validate_workflow(&wrong_name, platform).is_err());
            let mut missing_runtime = document.clone();
            missing_runtime["jobs"]["release"]["env"]["RUNTIME_BUILD"] = serde_yaml::Value::String("{}".into());
            assert!(validate_workflow(&missing_runtime, platform).is_err());
            let mut semver = document.clone();
            semver["jobs"]["prepare"]["steps"][0]["with"]["script"] = serde_yaml::Value::String("cfg.nextDeliveryVersion('wasm', [], [], '1.0.0')".into());
            assert!(validate_workflow(&semver, platform).is_err());
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

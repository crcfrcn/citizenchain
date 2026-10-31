use crate::update::github_tag_path;
use crate::update::installer_asset_url;
use crate::update::installer_label;
use crate::update::installer_name;
use crate::update::installer_url;
use crate::update::release_proof;
use crate::update::version_parts;

#[test]
fn installer_name_and_url_bind_platform_version_and_encoded_path() {
    for (platform, suffix) in [
        ("macOS", "dmg"),
        ("Windows", "exe"),
        ("LinuxARM", "deb"),
        ("LinuxAMD", "deb"),
    ] {
        let name = installer_name(platform, "1.0.0").unwrap();
        assert_eq!(name, format!("citizenchain-{platform}-v1.0.0.{suffix}"));
        assert_eq!(
            installer_label(platform, "1.0.0").unwrap(),
            format!("公民链·{platform}·v1.0.0.{suffix}")
        );
        let tag = format!("公民链·{platform}·v1.0.0");
        let url = installer_url(&tag, &name).unwrap();
        assert_eq!(url.host_str(), Some("github.com"));
        assert_eq!(url.scheme(), "https");
        assert!(url.as_str().contains("%E5%85%AC%E6%B0%91%E9%93%BE%C2%B7"));
        assert!(url
            .as_str()
            .ends_with(&format!("/citizenchain-{platform}-v1.0.0.{suffix}")));
        assert_eq!(url.as_str(), format!("https://github.com/crcfrcn/citizenchain/releases/download/%E5%85%AC%E6%B0%91%E9%93%BE%C2%B7{platform}%C2%B7v1.0.0/{name}"));
        assert_eq!(url.query(), None);
        assert_eq!(url.fragment(), None);
        let api_path = github_tag_path(&tag).unwrap();
        assert!(api_path.starts_with("git/ref/tags/%E5%85%AC%E6%B0%91%E9%93%BE%C2%B7"));
        assert!(api_path.ends_with(&format!("{platform}%C2%B7v1.0.0")));
        assert!(!api_path.contains('?') && !api_path.contains('#'));
    }
    for version in [
        "",
        "1.0",
        "01.0.0",
        "1.0.0/other",
        "1.0.0?other",
        "1.0.0-rc1",
    ] {
        assert!(installer_name("macOS", version).is_err());
    }
    assert!(installer_name("wasm", "1.0.0").is_err());
}

#[test]
fn installer_asset_requires_one_complete_platform_package() {
    let name = installer_name("LinuxARM", "1.0.1").unwrap();
    let tag = "公民链·LinuxARM·v1.0.1";
    let url = installer_url(tag, &name).unwrap().to_string();
    let hash = "a".repeat(64);
    let label = installer_label("LinuxARM", "1.0.1").unwrap();
    let asset = serde_json::json!({"name":name,"label":label,"state":"uploaded","size":3,"digest":format!("sha256:{hash}"),"browser_download_url":url});
    let release = serde_json::json!({"assets":[asset.clone()]});
    let proof = serde_json::json!({"assets":[{"name":name,"label":label,"size":3,"sha256":hash}]});
    assert_eq!(
        installer_asset_url(&release, &proof, "LinuxARM", "1.0.1", tag).unwrap(),
        url
    );
    for assets in [
        serde_json::json!([]),
        serde_json::json!([asset.clone(), asset.clone()]),
    ] {
        assert!(installer_asset_url(
            &serde_json::json!({"assets":assets}),
            &proof,
            "LinuxARM",
            "1.0.1",
            tag
        )
        .is_err());
    }
    assert!(installer_asset_url(&release, &proof, "LinuxAMD", "1.0.1", tag).is_err());
    for field in ["name", "label"] {
        let mut wrong = release.clone();
        wrong["assets"][0][field] = "错误名称".into();
        assert!(installer_asset_url(&wrong, &proof, "LinuxARM", "1.0.1", tag).is_err());
        let mut wrong = proof.clone();
        wrong["assets"][0][field] = "错误名称".into();
        assert!(installer_asset_url(&release, &wrong, "LinuxARM", "1.0.1", tag).is_err());
    }
    let mut wrong = release.clone();
    wrong["assets"][0]["browser_download_url"] = "https://example.invalid/package.deb".into();
    assert!(installer_asset_url(&wrong, &proof, "LinuxARM", "1.0.1", tag).is_err());
    wrong = release.clone();
    wrong["assets"][0]["digest"] = format!("sha256:{}", "b".repeat(64)).into();
    assert!(installer_asset_url(&wrong, &proof, "LinuxARM", "1.0.1", tag).is_err());
}

#[test]
fn versions_use_numeric_order() {
    assert!(version_parts("1.0.10").unwrap() > version_parts("1.0.9").unwrap());
    assert!(version_parts("1.1.0").unwrap() > version_parts("1.0.99").unwrap());
    assert!(version_parts("18446744073709551616.0.0").is_err());
}

#[test]
fn latest_formal_release_uses_publication_time_and_current_platform() {
    let release = |id, tag: &str, date: &str, draft, prerelease| {
        serde_json::json!({
            "id":id,"tag_name":tag,"published_at":date,"draft":draft,"prerelease":prerelease
        })
    };
    let rows = crate::update::formal_releases(
        vec![
            release(
                999,
                "公民链·macOS·v1.0.1",
                "2026-01-01T00:00:00Z",
                false,
                false,
            ),
            release(
                1,
                "公民链·macOS·v1.0.2",
                "2026-02-01T00:00:00Z",
                false,
                false,
            ),
            release(
                2,
                "公民链·macOS·v1.0.3",
                "2026-03-01T00:00:00Z",
                true,
                false,
            ),
            release(
                3,
                "公民链·macOS·v1.0.4",
                "2026-04-01T00:00:00Z",
                false,
                true,
            ),
            release(4, "公民链·WASM·v9", "2026-05-01T00:00:00Z", false, false),
            release(
                5,
                "公民链·Windows·v1.0.5",
                "2026-06-01T00:00:00Z",
                false,
                false,
            ),
        ],
        "macOS",
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], 1);
}

#[test]
fn download_redirects_only_follow_github_https_assets() {
    use crate::update::release_download_url;
    for value in [
        "https://github.com/package",
        "https://release-assets.githubusercontent.com/package",
    ] {
        assert!(release_download_url(&value.parse().unwrap()));
    }
    for value in [
        "http://github.com/package",
        "https://github.com.evil.invalid/package",
        "https://user@github.com/package",
    ] {
        assert!(!release_download_url(&value.parse().unwrap()));
    }
}

#[test]
fn installation_handoff_waits_for_exit_and_keeps_paths_as_data() {
    use crate::update::{installation_script, powershell_quote, shell_quote};
    use std::path::Path;
    assert_eq!(shell_quote("a'b"), "'a'\"'\"'b'");
    assert_eq!(powershell_quote("a'b"), "'a''b'");
    let root = Path::new("/tmp/up date'case");
    for (os, executable) in [
        (
            "macos",
            "/Applications/CitizenChain.app/Contents/MacOS/citizenchain",
        ),
        ("linux", "/usr/bin/citizenchain"),
        ("windows", "C:\\Program Files\\citizenchain.exe"),
    ] {
        let script =
            installation_script(os, root, &root.join("package"), Path::new(executable), 4242)
                .unwrap();
        assert!(script.contains("4242"));
        assert!(script.contains("install-error.txt"));
        assert!(!script.contains("kill -9") && !script.contains("ExecutionPolicy"));
        if os != "windows" && cfg!(unix) {
            let mut child = std::process::Command::new("/bin/sh")
                .arg("-n")
                .stdin(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(script.as_bytes())
                .unwrap();
            assert!(child.wait().unwrap().success());
        }
    }
    assert!(installation_script(
        "macos",
        root,
        Path::new("package"),
        Path::new("/tmp/dev-node"),
        1
    )
    .is_err());
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

#[test]
fn incomplete_download_never_replaces_the_installable_package() {
    use std::fs::{read, remove_file, File};
    use std::io::Write;
    let root =
        std::path::PathBuf::from(std::env::var("CARGO_TARGET_DIR").expect("独立测试输出目录"));
    let partial = root.join("update-download.partial");
    let complete = root.join("update-download.complete");
    for (bytes, total, expected) in [
        (b"ab".as_slice(), 3, false),
        (b"abcd".as_slice(), 3, false),
        (b"abc".as_slice(), 3, true),
    ] {
        let mut file = File::create(&partial).unwrap();
        file.write_all(bytes).unwrap();
        let result =
            crate::update::finish_download(file, &partial, &complete, bytes.len() as u64, total);
        assert_eq!(result.is_ok(), expected);
        assert_eq!(complete.exists(), expected);
        if expected {
            assert_eq!(read(&complete).unwrap(), bytes);
            remove_file(&complete).unwrap();
        } else {
            assert!(partial.exists());
            remove_file(&partial).unwrap();
        }
    }
}

use crate::update::github_tag_path;
use crate::update::installer_asset_url;
use crate::update::installer_browser_command;
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
fn versions_and_browser_commands_keep_download_separate_from_installation() {
    assert!(version_parts("1.0.10").unwrap() > version_parts("1.0.9").unwrap());
    assert!(version_parts("1.1.0").unwrap() > version_parts("1.0.99").unwrap());
    assert_eq!(
        version_parts("1.0.0").unwrap(),
        version_parts("1.0.0").unwrap()
    );
    assert!(version_parts("18446744073709551616.0.0").is_err());
    let url = "https://github.com/crcfrcn/citizenchain/releases/download/version/package.deb";
    for (os, program, prefix) in [
        ("macos", "/usr/bin/open", None),
        (
            "windows",
            "rundll32.exe",
            Some("url.dll,FileProtocolHandler"),
        ),
        ("linux", "xdg-open", None),
    ] {
        let command = installer_browser_command(os, url).unwrap();
        assert_eq!(command.get_program(), program);
        let mut arguments = command.get_args();
        if let Some(prefix) = prefix {
            assert_eq!(arguments.next().unwrap(), prefix);
        }
        assert_eq!(arguments.next().unwrap(), url);
        assert!(arguments.next().is_none());
    }
    assert!(installer_browser_command("other", url).is_err());
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

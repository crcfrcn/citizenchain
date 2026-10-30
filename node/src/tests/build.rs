use super::{comparison_path, local_build_work, path_within, same_path};
use std::path::{Path, PathBuf};

#[test]
fn comparison_keeps_directory_boundaries_and_different_targets() {
    let root = PathBuf::from(if cfg!(windows) {
        r"C:\repo\target\build\windows"
    } else {
        "/repo/target/build/windows"
    });
    assert!(path_within(&root.join("work/out"), &root));
    assert!(!path_within(
        &root.with_file_name("windows-other").join("out"),
        &root
    ));
    assert!(!same_path(&root.join("work/../out"), &root.join("out")));
    assert!(!same_path(&root.join("link/out"), &root.join("real/out")));
}

#[test]
fn local_macos_work_is_limited_to_the_compiler_generated_directory() {
    let root = PathBuf::from(if cfg!(windows) { r"C:\repo" } else { "/repo" });
    let build = root.join("target/build");
    assert!(local_build_work(
        &root,
        &build.join("local-Abc123"),
        "macos"
    ));
    assert!(!local_build_work(
        &root,
        &build.join("local-Abc123"),
        "windows"
    ));
    assert!(!local_build_work(
        &root,
        &build.join("local-Abc123/nested"),
        "macos"
    ));
    assert!(!local_build_work(
        &root,
        &root.join("target/build-other/local-Abc123"),
        "macos"
    ));
    for name in [
        "local-",
        "local-short",
        "local-toolong",
        "local-中文汉字",
        "other-Abc123",
    ] {
        assert!(!local_build_work(&root, &build.join(name), "macos"));
    }
}

#[cfg(windows)]
#[test]
fn windows_disk_and_unc_prefixes_compare_without_relaxing_ownership() {
    assert!(same_path(
        Path::new(r"C:\repo\target\out"),
        Path::new(r"\\?\C:\repo\target\out")
    ));
    assert!(same_path(
        Path::new(r"c:\repo\target\out"),
        Path::new(r"\\?\C:\repo\target\out")
    ));
    assert!(same_path(
        Path::new(r"\\server\share\repo\out"),
        Path::new(r"\\?\UNC\server\share\repo\out")
    ));
    assert!(path_within(
        Path::new(r"\\?\C:\repo\target\out"),
        Path::new(r"C:\repo\target")
    ));
    assert!(!same_path(
        Path::new(r"D:\repo\out"),
        Path::new(r"\\?\C:\repo\out")
    ));
    assert!(!same_path(
        Path::new(r"C:\repo\out"),
        Path::new(r"\\.\C:\repo\out")
    ));
    assert!(!path_within(
        Path::new(r"\\?\C:\repo\target-other\out"),
        Path::new(r"C:\repo\target")
    ));
    assert!(!same_path(
        Path::new(r"C:\repo\link\out"),
        Path::new(r"\\?\C:\repo\real\out")
    ));
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

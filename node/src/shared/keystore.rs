// 通用 keystore 操作：扫描链目录、写入/删除/检测密钥文件。
#![allow(unsafe_code)]
// 本模块需要用 openat/renameat/fstatat 等 Unix 原子文件接口保证 keystore 写入安全。
use crate::shared::security;
use std::{
    ffi::OsString,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};
use tauri::AppHandle;

#[cfg(unix)]
use std::{
    ffi::{CStr, CString, OsStr},
    io::Write,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::ffi::{OsStrExt, OsStringExt},
    },
    time::{SystemTime, UNIX_EPOCH},
};

const DEFAULT_CHAIN_ID: &str = "citizenchain";

/// 返回节点数据根目录 `<app_data>`，不存在时自动创建。
pub(crate) fn node_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let path = security::app_data_dir(app)?;
    ensure_directory_secure(&path)
        .map_err(|e| format!("create node data dir failed ({}): {e}", path.display()))?;
    Ok(path)
}

/// 扫描 `<app_data>/chains/*/keystore` 目录列表，始终包含默认链 ID 对应的目录。
/// 跳过符号链接，确保 keystore 目录已创建。
pub(crate) fn keystore_dirs(app: &AppHandle) -> Result<Vec<PathBuf>, String> {
    let chains_root = node_data_dir(app)?.join("chains");
    ensure_directory_secure(&chains_root)
        .map_err(|e| format!("create chains dir failed ({}): {e}", chains_root.display()))?;

    let mut dirs: Vec<PathBuf> = Vec::new();
    for chain_name in child_directory_names(&chains_root)? {
        let chain_dir = chains_root.join(&chain_name);
        ensure_directory_secure(&chain_dir)
            .map_err(|e| format!("create chain dir failed ({}): {e}", chain_dir.display()))?;
        let candidate = chain_dir.join("keystore");
        ensure_directory_secure(&candidate)
            .map_err(|e| format!("create keystore dir failed ({}): {e}", candidate.display()))?;
        dirs.push(candidate);
    }

    let default_chain = chains_root.join(DEFAULT_CHAIN_ID);
    ensure_directory_secure(&default_chain)
        .map_err(|e| format!("create chain dir failed ({}): {e}", default_chain.display()))?;
    let default_ks = default_chain.join("keystore");
    ensure_directory_secure(&default_ks)
        .map_err(|e| format!("create keystore dir failed ({}): {e}", default_ks.display()))?;
    dirs.push(default_ks);
    dirs.sort();
    dirs.dedup();

    Ok(dirs)
}

/// 根据密钥类型前缀和公钥生成 keystore 文件名。
pub(crate) fn keystore_filename(key_type_prefix: &str, public_key: &str) -> String {
    format!("{key_type_prefix}{public_key}")
}

/// 扫描所有 keystore 目录，返回匹配指定前缀的文件路径列表。
#[allow(dead_code)]
pub(crate) fn scan_keystore_files(
    dirs: &[PathBuf],
    key_type_prefix: &str,
) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for dir in dirs {
        for name in regular_file_names_with_prefix(dir, key_type_prefix)? {
            let path = dir.join(name);
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// 将密钥写入所有 keystore 目录，并移除同类型的其他旧密钥文件。
pub(crate) fn write_key_to_keystore(
    dirs: &[PathBuf],
    key_type_prefix: &str,
    public_key: &str,
    secret_content: &str,
) -> Result<(), String> {
    let filename = keystore_filename(key_type_prefix, public_key);
    for dir in dirs {
        write_secret_text_atomic_secure(dir, &filename, secret_content).map_err(|e| {
            format!(
                "write keystore file failed ({}/{}): {e}",
                dir.display(),
                filename
            )
        })?;
    }
    remove_other_keys(dirs, key_type_prefix, &filename)?;
    Ok(())
}

/// 将密钥写入所有 keystore 目录，同时保留同类型其它密钥。
///
/// GRANDPA 正常更换必须在 authority set 生效前同时保留旧、新私钥，因此不能调用
/// 会清理同类型旧文件的 [`write_key_to_keystore`]。
pub(crate) fn write_key_to_keystore_preserving_others(
    dirs: &[PathBuf],
    key_type_prefix: &str,
    public_key: &str,
    secret_content: &str,
) -> Result<(), String> {
    let filename = keystore_filename(key_type_prefix, public_key);
    for dir in dirs {
        write_secret_text_atomic_secure(dir, &filename, secret_content).map_err(|e| {
            format!(
                "write keystore file failed ({}/{}): {e}",
                dir.display(),
                filename
            )
        })?;
    }
    Ok(())
}

/// 从所有 keystore 目录删除一把准确的公钥文件，不影响其它同类型密钥。
pub(crate) fn remove_key_from_keystore(
    dirs: &[PathBuf],
    key_type_prefix: &str,
    public_key: &str,
) -> Result<(), String> {
    let filename = keystore_filename(key_type_prefix, public_key);
    for dir in dirs {
        remove_file_secure(dir, &filename).map_err(|e| {
            format!(
                "remove keystore file failed ({}/{}): {e}",
                dir.display(),
                filename
            )
        })?;
    }
    Ok(())
}

/// 读取准确公钥对应的 keystore 文本；多目录内容必须完全一致。
pub(crate) fn read_key_from_keystore(
    dirs: &[PathBuf],
    key_type_prefix: &str,
    public_key: &str,
) -> Result<Option<String>, String> {
    let filename = keystore_filename(key_type_prefix, public_key);
    let mut found: Option<String> = None;
    for dir in dirs {
        if !regular_file_exists_secure(dir, &filename)? {
            continue;
        }
        let value = fs::read_to_string(dir.join(&filename))
            .map_err(|e| format!("read keystore file failed ({filename}): {e}"))?;
        if found.as_ref().is_some_and(|current| current != &value) {
            return Err(format!(
                "同一 GRANDPA 公钥在多个 keystore 目录中的私钥内容不一致: {public_key}"
            ));
        }
        found = Some(value);
    }
    Ok(found)
}

/// 移除 keystore 中同类型但不匹配 keep_filename 的旧密钥文件。
pub(crate) fn remove_other_keys(
    dirs: &[PathBuf],
    key_type_prefix: &str,
    keep_filename: &str,
) -> Result<(), String> {
    for dir in dirs {
        for name in regular_file_names_with_prefix(dir, key_type_prefix)? {
            if name == keep_filename {
                continue;
            }
            remove_file_secure(dir, &name).map_err(|e| {
                format!(
                    "remove stale keystore file failed ({}/{}): {e}",
                    dir.display(),
                    name
                )
            })?;
        }
    }
    Ok(())
}

/// 返回默认链（citizenchain）的 keystore 目录路径。
/// 仅扫描默认链目录，避免旧链残留 keystore 干扰矿工身份判定。
pub(crate) fn default_chain_keystore_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let ks = node_data_dir(app)?
        .join("chains")
        .join(DEFAULT_CHAIN_ID)
        .join("keystore");
    ensure_directory_secure(&ks).map_err(|e| {
        format!(
            "create default chain keystore dir failed ({}): {e}",
            ks.display()
        )
    })?;
    Ok(ks)
}

/// 检查指定公钥的 keystore 文件是否存在于任意 keystore 目录中。
pub(crate) fn has_key_in_keystore(
    dirs: &[PathBuf],
    key_type_prefix: &str,
    public_key: &str,
) -> bool {
    let filename = keystore_filename(key_type_prefix, public_key);
    dirs.iter()
        .any(|dir| regular_file_exists_secure(dir, &filename).unwrap_or(false))
}

#[cfg(unix)]
fn ensure_directory_secure(path: &Path) -> Result<(), String> {
    let base_path = if path.is_absolute() {
        PathBuf::from("/")
    } else {
        std::env::current_dir().map_err(|e| format!("resolve current dir failed: {e}"))?
    };
    let mut current_path = base_path.clone();
    let mut current_fd = open_dir_nofollow(&base_path)?;

    for component in path.components() {
        let name = match component {
            std::path::Component::RootDir | std::path::Component::CurDir => continue,
            std::path::Component::Normal(name) => name,
            std::path::Component::ParentDir => {
                return Err(format!(
                    "reject keystore path containing parent component: {}",
                    path.display()
                ))
            }
            std::path::Component::Prefix(_) => {
                return Err(format!(
                    "unsupported keystore path prefix: {}",
                    path.display()
                ))
            }
        };

        let cname = cstring_from_os_str(name)?;
        let rc = unsafe { libc::mkdirat(current_fd.as_raw_fd(), cname.as_ptr(), 0o700) };
        let created = rc == 0;
        if rc < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() != ErrorKind::AlreadyExists {
                return Err(format!("mkdirat failed: {err}"));
            }
        }

        current_path.push(name);
        let next_fd = open_dir_at(current_fd.as_raw_fd(), name)?;
        if created || current_path == path {
            set_dir_permissions_0700(next_fd.as_raw_fd(), &current_path)?;
        }
        current_fd = next_fd;
    }

    Ok(())
}

#[cfg(not(unix))]
fn ensure_directory_secure(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| e.to_string())
}

#[cfg(unix)]
fn child_directory_names(dir: &Path) -> Result<Vec<OsString>, String> {
    let dir_fd = open_dir_nofollow(dir)?;
    let mut names = Vec::new();
    for entry in read_dir_names(dir_fd.as_raw_fd())? {
        if matches!(
            entry_kind(dir_fd.as_raw_fd(), &entry)?,
            Some(EntryKind::Directory)
        ) {
            names.push(entry);
        }
    }
    Ok(names)
}

#[cfg(not(unix))]
fn child_directory_names(dir: &Path) -> Result<Vec<OsString>, String> {
    let mut names = Vec::new();
    let entries = fs::read_dir(dir).map_err(|e| format!("read dir failed: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("read dir entry failed: {e}"))?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("read dir entry type failed: {e}"))?;
        if !file_type.is_symlink() && file_type.is_dir() {
            names.push(entry.file_name());
        }
    }
    Ok(names)
}

#[cfg(unix)]
fn regular_file_names_with_prefix(dir: &Path, prefix: &str) -> Result<Vec<String>, String> {
    let dir_fd = open_dir_nofollow(dir)?;
    let mut names = Vec::new();
    for entry in read_dir_names(dir_fd.as_raw_fd())? {
        let Some(name) = entry.to_str() else {
            continue;
        };
        if !name.starts_with(prefix) {
            continue;
        }
        if matches!(
            entry_kind(dir_fd.as_raw_fd(), &entry)?,
            Some(EntryKind::File)
        ) {
            names.push(name.to_string());
        }
    }
    Ok(names)
}

#[cfg(not(unix))]
fn regular_file_names_with_prefix(dir: &Path, prefix: &str) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    let entries = fs::read_dir(dir).map_err(|e| format!("read keystore dir failed: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("read keystore entry failed: {e}"))?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("read keystore file type failed: {e}"))?;
        if file_type.is_symlink() || !file_type.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(|v| v.to_string()) else {
            continue;
        };
        if name.starts_with(prefix) {
            names.push(name);
        }
    }
    Ok(names)
}

#[cfg(unix)]
fn write_secret_text_atomic_secure(
    dir: &Path,
    filename: &str,
    content: &str,
) -> Result<(), String> {
    let dir_fd = open_dir_nofollow(dir)?;
    let target = cstring_from_str(filename)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_nanos())
        .unwrap_or(0);
    let random_suffix = rand::random::<u64>();

    for seq in 0..8u8 {
        let temp_name = format!(
            ".{filename}.tmp-{}-{stamp}-{random_suffix}-{seq}",
            std::process::id()
        );
        let temp = cstring_from_str(&temp_name)?;
        let raw_fd = unsafe {
            libc::openat(
                dir_fd.as_raw_fd(),
                temp.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                0o600,
            )
        };
        if raw_fd < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == ErrorKind::AlreadyExists {
                continue;
            }
            return Err(format!("openat temp file failed: {err}"));
        }

        let mut file = unsafe { fs::File::from_raw_fd(raw_fd) };
        if let Err(err) = set_file_permissions_0600(file.as_raw_fd()) {
            let _ = remove_file_at(dir_fd.as_raw_fd(), &temp_name);
            return Err(err);
        }
        if let Err(err) = file.write_all(content.as_bytes()) {
            let _ = remove_file_at(dir_fd.as_raw_fd(), &temp_name);
            return Err(format!("write temp keystore file failed: {err}"));
        }
        if let Err(err) = file.sync_all() {
            let _ = remove_file_at(dir_fd.as_raw_fd(), &temp_name);
            return Err(format!("sync temp keystore file failed: {err}"));
        }
        drop(file);

        let rename_rc = unsafe {
            libc::renameat(
                dir_fd.as_raw_fd(),
                temp.as_ptr(),
                dir_fd.as_raw_fd(),
                target.as_ptr(),
            )
        };
        if rename_rc < 0 {
            let err = std::io::Error::last_os_error();
            let _ = remove_file_at(dir_fd.as_raw_fd(), &temp_name);
            return Err(format!("rename temp keystore file failed: {err}"));
        }

        let sync_rc = unsafe { libc::fsync(dir_fd.as_raw_fd()) };
        if sync_rc < 0 {
            return Err(format!(
                "sync keystore directory failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        return Ok(());
    }

    Err("create temp keystore file failed: exhausted retries".to_string())
}

#[cfg(not(unix))]
fn write_secret_text_atomic_secure(
    dir: &Path,
    filename: &str,
    content: &str,
) -> Result<(), String> {
    security::write_secret_text_atomic(&dir.join(filename), content)
}

#[cfg(unix)]
fn remove_file_secure(dir: &Path, filename: &str) -> Result<(), String> {
    let dir_fd = open_dir_nofollow(dir)?;
    remove_file_at(dir_fd.as_raw_fd(), filename)
}

#[cfg(not(unix))]
fn remove_file_secure(dir: &Path, filename: &str) -> Result<(), String> {
    match fs::remove_file(dir.join(filename)) {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.to_string()),
    }
}

#[cfg(unix)]
fn regular_file_exists_secure(dir: &Path, filename: &str) -> Result<bool, String> {
    let dir_fd = open_dir_nofollow(dir)?;
    Ok(matches!(
        entry_kind(dir_fd.as_raw_fd(), OsStr::new(filename))?,
        Some(EntryKind::File)
    ))
}

#[cfg(not(unix))]
fn regular_file_exists_secure(dir: &Path, filename: &str) -> Result<bool, String> {
    Ok(dir.join(filename).is_file())
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntryKind {
    Directory,
    File,
    Other,
}

#[cfg(unix)]
fn entry_kind(dir_fd: RawFd, name: &OsStr) -> Result<Option<EntryKind>, String> {
    let cname = cstring_from_os_str(name)?;
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    let rc = unsafe {
        libc::fstatat(
            dir_fd,
            cname.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if rc < 0 {
        let err = std::io::Error::last_os_error();
        if err.kind() == ErrorKind::NotFound {
            return Ok(None);
        }
        return Err(format!("fstatat failed: {err}"));
    }
    let stat = unsafe { stat.assume_init() };
    let kind = match stat.st_mode & libc::S_IFMT {
        libc::S_IFDIR => EntryKind::Directory,
        libc::S_IFREG => EntryKind::File,
        _ => EntryKind::Other,
    };
    Ok(Some(kind))
}

#[cfg(unix)]
fn read_dir_names(dir_fd: RawFd) -> Result<Vec<OsString>, String> {
    let dup_fd = unsafe { libc::fcntl(dir_fd, libc::F_DUPFD_CLOEXEC, 0) };
    if dup_fd < 0 {
        return Err(format!(
            "duplicate dir fd failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    let dir_ptr = unsafe { libc::fdopendir(dup_fd) };
    if dir_ptr.is_null() {
        let err = std::io::Error::last_os_error();
        unsafe {
            libc::close(dup_fd);
        }
        return Err(format!("fdopendir failed: {err}"));
    }

    let mut entries = Vec::new();
    loop {
        let entry = unsafe { libc::readdir(dir_ptr) };
        if entry.is_null() {
            break;
        }
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        let bytes = name.to_bytes();
        if bytes == b"." || bytes == b".." {
            continue;
        }
        entries.push(OsString::from_vec(bytes.to_vec()));
    }

    if unsafe { libc::closedir(dir_ptr) } < 0 {
        return Err(format!(
            "closedir failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    Ok(entries)
}

#[cfg(unix)]
fn open_dir_nofollow(path: &Path) -> Result<OwnedFd, String> {
    let cpath = cstring_from_os_str(path.as_os_str())?;
    let raw_fd = unsafe {
        libc::open(
            cpath.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if raw_fd < 0 {
        return Err(format!(
            "open dir failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(unsafe { OwnedFd::from_raw_fd(raw_fd) })
}

#[cfg(unix)]
fn open_dir_at(parent_fd: RawFd, name: &OsStr) -> Result<OwnedFd, String> {
    let cname = cstring_from_os_str(name)?;
    let raw_fd = unsafe {
        libc::openat(
            parent_fd,
            cname.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if raw_fd < 0 {
        return Err(format!(
            "open child dir failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(unsafe { OwnedFd::from_raw_fd(raw_fd) })
}

#[cfg(unix)]
fn remove_file_at(dir_fd: RawFd, filename: &str) -> Result<(), String> {
    let cname = cstring_from_str(filename)?;
    let rc = unsafe { libc::unlinkat(dir_fd, cname.as_ptr(), 0) };
    if rc < 0 {
        let err = std::io::Error::last_os_error();
        if err.kind() == ErrorKind::NotFound {
            return Ok(());
        }
        return Err(format!("unlinkat failed: {err}"));
    }
    Ok(())
}

#[cfg(unix)]
fn set_dir_permissions_0700(fd: RawFd, path: &Path) -> Result<(), String> {
    let rc = unsafe { libc::fchmod(fd, 0o700) };
    if rc < 0 {
        return Err(format!(
            "set dir permission failed ({}): {}",
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn set_file_permissions_0600(fd: RawFd) -> Result<(), String> {
    let rc = unsafe { libc::fchmod(fd, 0o600) };
    if rc < 0 {
        return Err(format!(
            "set file permission failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn cstring_from_os_str(value: &OsStr) -> Result<CString, String> {
    CString::new(value.as_bytes()).map_err(|_| "keystore path contains NUL byte".to_string())
}

#[cfg(unix)]
fn cstring_from_str(value: &str) -> Result<CString, String> {
    CString::new(value).map_err(|_| "keystore filename contains NUL byte".to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_directory_secure, has_key_in_keystore, read_key_from_keystore,
        remove_key_from_keystore, write_key_to_keystore, write_key_to_keystore_preserving_others,
    };
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|v| v.as_nanos())
            .unwrap_or(0);
        std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!("node-keystore-{label}-{stamp}"))
    }

    #[test]
    fn ensure_directory_secure_creates_path() {
        let path = unique_temp_dir("create")
            .join("chains")
            .join("citizenchain")
            .join("keystore");
        ensure_directory_secure(&path).unwrap();
        assert!(path.is_dir());
        let _ = fs::remove_dir_all(path.ancestors().nth(3).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn ensure_directory_secure_sets_permissions_to_0700() {
        let path = unique_temp_dir("mode")
            .join("chains")
            .join("citizenchain")
            .join("keystore");
        ensure_directory_secure(&path).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        let _ = fs::remove_dir_all(path.ancestors().nth(3).unwrap());
    }

    #[test]
    fn grandpa_rotation_preserves_both_keys_then_removes_only_old_key() {
        let root = unique_temp_dir("grandpa-rotation");
        let keystore = root.join("chains").join("citizenchain").join("keystore");
        ensure_directory_secure(&keystore).unwrap();
        let dirs = vec![keystore];
        let old_public_key = "11".repeat(32);
        let new_public_key = "22".repeat(32);

        write_key_to_keystore(&dirs, "6772616e", &old_public_key, "\"0xold\"\n").unwrap();
        write_key_to_keystore_preserving_others(&dirs, "6772616e", &new_public_key, "\"0xnew\"\n")
            .unwrap();
        assert!(has_key_in_keystore(&dirs, "6772616e", &old_public_key));
        assert!(has_key_in_keystore(&dirs, "6772616e", &new_public_key));
        assert_eq!(
            read_key_from_keystore(&dirs, "6772616e", &new_public_key).unwrap(),
            Some("\"0xnew\"\n".to_string())
        );

        remove_key_from_keystore(&dirs, "6772616e", &old_public_key).unwrap();
        assert!(!has_key_in_keystore(&dirs, "6772616e", &old_public_key));
        assert!(has_key_in_keystore(&dirs, "6772616e", &new_public_key));
        let _ = fs::remove_dir_all(root);
    }
}

//! 组织机构资料库路由并校验上传正文和文件大小。

#[path = "handler.rs"]
pub mod handler;

/// 公民和机构资料共用文件上限；请求体额外允许有界 multipart 元数据。
pub(crate) const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_REQUEST_BYTES: usize = MAX_FILE_BYTES + 64 * 1024;

pub fn upload_body_limit() -> axum::extract::DefaultBodyLimit {
    axum::extract::DefaultBodyLimit::max(MAX_REQUEST_BYTES)
}

pub fn valid_file_size(size: usize) -> bool {
    (1..=MAX_FILE_BYTES).contains(&size)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/documents.rs"]
mod tests;

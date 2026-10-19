//! 机构资料库模块。
//!
//! 资料上传、下载、删除和列表从机构聚合模块拆出路由归属。

#[path = "handler.rs"]
pub(crate) mod handler;

/// 公民和机构资料共用文件上限；请求体额外允许有界 multipart 元数据。
pub(crate) const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;
pub(crate) const MAX_REQUEST_BYTES: usize = MAX_FILE_BYTES + 64 * 1024;

pub(crate) fn upload_body_limit() -> axum::extract::DefaultBodyLimit {
    axum::extract::DefaultBodyLimit::max(MAX_REQUEST_BYTES)
}

pub(crate) fn valid_file_size(size: usize) -> bool {
    (1..=MAX_FILE_BYTES).contains(&size)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn file_limit_accepts_large_files_and_rejects_empty_or_oversize() {
        assert!(!valid_file_size(0));
        assert!(valid_file_size(2 * 1024 * 1024 + 1));
        assert!(valid_file_size(MAX_FILE_BYTES));
        assert!(!valid_file_size(MAX_FILE_BYTES + 1));
        assert!(MAX_REQUEST_BYTES > MAX_FILE_BYTES);
    }
    #[tokio::test]
    async fn multipart_accepts_large_boundary_and_rejects_oversize_or_broken_stream() {
        use axum::{extract::Multipart, http::{Request, StatusCode}, routing::post, Router};
        use tower::ServiceExt;
        async fn receive(mut body: Multipart) -> StatusCode {
            loop {
                let field = match body.next_field().await {
                    Ok(Some(field)) => field, Ok(None) => return StatusCode::OK,
                    Err(error) => return error.status(),
                };
                let bytes = match field.bytes().await { Ok(bytes) => bytes, Err(error) => return error.status() };
                if !valid_file_size(bytes.len()) { return StatusCode::PAYLOAD_TOO_LARGE; }
            }
        }
        for (size, complete, expected) in [
            (2*1024*1024+1, true, StatusCode::OK), (MAX_FILE_BYTES, true, StatusCode::OK),
            (MAX_FILE_BYTES+1, true, StatusCode::PAYLOAD_TOO_LARGE),
            (MAX_REQUEST_BYTES+1, true, StatusCode::PAYLOAD_TOO_LARGE),
            (100, false, StatusCode::BAD_REQUEST),
        ] {
            let mut body = b"--boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"sample.bin\"\r\n\r\n".to_vec();
            body.resize(body.len()+size, b'x');
            if complete { body.extend_from_slice(b"\r\n--boundary--\r\n"); }
            let router = Router::new().route("/upload", post(receive)).layer(upload_body_limit());
            let response = router.oneshot(Request::builder().uri("/upload").method("POST").header("content-type", "multipart/form-data; boundary=boundary").body(axum::body::Body::from(body)).unwrap()).await.unwrap();
            assert_eq!(response.status(), expected, "size={size} complete={complete}");
        }
    }

}

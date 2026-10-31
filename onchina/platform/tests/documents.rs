//! 原所属模块回归，断言与输入保持。
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
    use axum::{
        extract::Multipart,
        http::{Request, StatusCode},
        routing::post,
        Router,
    };
    use tower::ServiceExt;
    async fn receive(mut body: Multipart) -> StatusCode {
        loop {
            let field = match body.next_field().await {
                Ok(Some(field)) => field,
                Ok(None) => return StatusCode::OK,
                Err(error) => return error.status(),
            };
            let bytes = match field.bytes().await {
                Ok(bytes) => bytes,
                Err(error) => return error.status(),
            };
            if !valid_file_size(bytes.len()) {
                return StatusCode::PAYLOAD_TOO_LARGE;
            }
        }
    }
    for (size, complete, expected) in [
        (2 * 1024 * 1024 + 1, true, StatusCode::OK),
        (MAX_FILE_BYTES, true, StatusCode::OK),
        (MAX_FILE_BYTES + 1, true, StatusCode::PAYLOAD_TOO_LARGE),
        (MAX_REQUEST_BYTES + 1, true, StatusCode::PAYLOAD_TOO_LARGE),
        (100, false, StatusCode::BAD_REQUEST),
    ] {
        let mut body = b"--boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"sample.bin\"\r\n\r\n".to_vec();
        body.resize(body.len() + size, b'x');
        if complete {
            body.extend_from_slice(b"\r\n--boundary--\r\n");
        }
        let router = Router::new()
            .route("/upload", post(receive))
            .layer(upload_body_limit());
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/upload")
                    .method("POST")
                    .header("content-type", "multipart/form-data; boundary=boundary")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            expected,
            "size={size} complete={complete}"
        );
    }
}

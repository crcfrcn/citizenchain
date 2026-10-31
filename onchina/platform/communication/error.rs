// 保留原有状态码、业务错误码、提示和追踪字段。
pub fn api_error(status: StatusCode, code: u32, message: &str) -> axum::response::Response {
    (
        status,
        Json(ApiError {
            code,
            error_code: onchina_error_code(status, message),
            message: message.to_string(),
            trace_id: Uuid::new_v4().to_string(),
        }),
    )
        .into_response()
}

fn onchina_error_code(status: StatusCode, message: &str) -> &'static str {
    // HTTP 状态表达协议层含义,稳定 error_code 表达业务语义;前端不得解析 message。
    match message {
        "missing bearer token" => "ONCHINA_AUTH_MISSING_TOKEN",
        "invalid access token" => "ONCHINA_AUTH_INVALID_ACCESS_TOKEN",
        "access token expired" => "ONCHINA_AUTH_ACCESS_TOKEN_EXPIRED",
        "admin disabled" => "ONCHINA_AUTH_ADMIN_DISABLED",
        "permission denied" => "ONCHINA_AUTH_PERMISSION_DENIED",
        "origin is required" => "ONCHINA_LOGIN_ORIGIN_REQUIRED",
        "session_id is required" => "ONCHINA_LOGIN_SESSION_REQUIRED",
        "domain is required" => "ONCHINA_LOGIN_DOMAIN_REQUIRED",
        "challenge_id, account_id, signature are required" => "ONCHINA_LOGIN_REQUEST_INVALID",
        "challenge_id and session_id are required" => "ONCHINA_LOGIN_RESULT_PARAM_REQUIRED",
        "sign request not found" => "ONCHINA_LOGIN_CHALLENGE_NOT_FOUND",
        "sign request already consumed" => "ONCHINA_LOGIN_CHALLENGE_CONSUMED",
        "sign request session mismatch" => "ONCHINA_LOGIN_SESSION_MISMATCH",
        "sign request expired" => "ONCHINA_LOGIN_CHALLENGE_EXPIRED",
        "account_id must match targeted account_id" => "ONCHINA_LOGIN_SIGNER_MISMATCH",
        "login signature verify failed" => "ONCHINA_LOGIN_SIGNATURE_VERIFY_FAILED",
        "chain unreachable" => "ONCHINA_LOGIN_CHAIN_UNREACHABLE",
        "desktop governance institution is not supported by OnChina" => {
            "ONCHINA_LOGIN_DESKTOP_GOVERNANCE_UNSUPPORTED"
        }
        "personal multisig is not supported by OnChina" => {
            "ONCHINA_LOGIN_PERSONAL_MULTISIG_UNSUPPORTED"
        }
        "node binding required" => "ONCHINA_LOGIN_NODE_BINDING_REQUIRED",
        "node binding missing" => "ONCHINA_LOGIN_NODE_BINDING_MISSING",
        "node binding invalid" => "ONCHINA_LOGIN_NODE_BINDING_INVALID",
        "node binding query failed" => "ONCHINA_LOGIN_NODE_BINDING_QUERY_FAILED",
        "node binding already inactive" => "ONCHINA_LOGIN_NODE_BINDING_ALREADY_INACTIVE",
        "node binding challenge not found" => "ONCHINA_LOGIN_NODE_BINDING_CHALLENGE_NOT_FOUND",
        "node binding challenge already consumed" => {
            "ONCHINA_LOGIN_NODE_BINDING_CHALLENGE_CONSUMED"
        }
        "node binding challenge expired" => "ONCHINA_LOGIN_NODE_BINDING_CHALLENGE_EXPIRED",
        "binding_challenge_id and candidate_id are required" => {
            "ONCHINA_LOGIN_NODE_BINDING_REQUEST_INVALID"
        }
        "selected institution candidate not found" => {
            "ONCHINA_LOGIN_NODE_BINDING_CANDIDATE_NOT_FOUND"
        }
        "admin no longer belongs to selected institution" => {
            "ONCHINA_LOGIN_NODE_BINDING_ADMIN_MISMATCH"
        }
        "login state error" => "ONCHINA_LOGIN_STATE_ERROR",
        _ if message.starts_with("authorization scope unavailable") => {
            "ONCHINA_LOGIN_SCOPE_UNAVAILABLE"
        }
        "challenge account_id mismatch" => "ONCHINA_BIND_ACCOUNT_ID_MISMATCH",
        "signature verify failed" => "ONCHINA_BIND_SIGNATURE_VERIFY_FAILED",
        "当前注册局缺少机构主账户绑定" => "ONCHINA_REGISTRY_MAIN_ACCOUNT_MISSING",
        "onchina ca certificate unavailable" => "ONCHINA_TLS_CA_UNAVAILABLE",
        "admin account_id already exists as federal admin" => {
            "ONCHINA_ACCOUNT_ID_EXISTS_AS_FEDERAL_REGISTRY_ADMIN"
        }
        "admin account_id already exists as city admin" => {
            "ONCHINA_ACCOUNT_ID_EXISTS_AS_CITY_REGISTRY_ADMIN"
        }
        "city admin city limit reached" => "ONCHINA_ADMIN_CITY_REGISTRY_CITY_LIMIT_REACHED",
        "not an on-chain admin" => "ONCHINA_LOGIN_ADMIN_NOT_ONCHAIN",
        "security grant required" => "ONCHINA_ADMIN_SECURITY_GRANT_REQUIRED",
        _ if message.starts_with("insert qr sign request failed") => {
            "ONCHINA_LOGIN_CHALLENGE_CREATE_FAILED"
        }
        _ if message.starts_with("identity_qr must be a complete QR_V1 account_id_code") => {
            "ONCHINA_LOGIN_WALLET_CODE_INVALID"
        }
        _ if message.starts_with("complete qr login failed") => "ONCHINA_LOGIN_COMPLETE_FAILED",
        _ if message.starts_with("persist qr login result failed") => {
            "ONCHINA_LOGIN_RESULT_SAVE_FAILED"
        }
        _ if message.starts_with("query qr login result failed") => {
            "ONCHINA_LOGIN_RESULT_QUERY_FAILED"
        }
        _ if status == StatusCode::UNAUTHORIZED => "ONCHINA_AUTH_UNAUTHORIZED",
        _ if status == StatusCode::FORBIDDEN => "ONCHINA_AUTH_FORBIDDEN",
        _ if status == StatusCode::BAD_REQUEST => "ONCHINA_REQUEST_INVALID",
        _ if status == StatusCode::NOT_FOUND => "ONCHINA_RESOURCE_NOT_FOUND",
        _ if status == StatusCode::CONFLICT => "ONCHINA_RESOURCE_CONFLICT",
        _ if status == StatusCode::GONE => "ONCHINA_RESOURCE_EXPIRED",
        _ if status == StatusCode::UNPROCESSABLE_ENTITY => "ONCHINA_BUSINESS_VALIDATION_FAILED",
        _ if status == StatusCode::TOO_MANY_REQUESTS => "ONCHINA_RATE_LIMITED",
        _ if status == StatusCode::SERVICE_UNAVAILABLE => "ONCHINA_SERVICE_UNAVAILABLE",
        _ => "ONCHINA_INTERNAL_ERROR",
    }
}

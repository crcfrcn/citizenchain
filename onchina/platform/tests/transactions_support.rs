//! 通用交易会话测试辅助，只在测试模式链接。
use super::*;
use chrono::Duration;
pub fn chain_session_with_context(context: serde_json::Value) -> ChainSignSession {
    ChainSignSession {
        request_id: "request-1".to_string(),
        purpose: builtin_purposes()[0].to_string(),
        account_id: "0x1111111111111111111111111111111111111111111111111111111111111111"
            .to_string(),
        call_data: vec![],
        nonce: 3,
        signing_hash: "aa".repeat(32),
        context,
        expires_at: Utc::now() + Duration::minutes(10),
        consumed_at: None,
    }
}

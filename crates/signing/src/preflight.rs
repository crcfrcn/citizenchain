//! 预检错误分类与原有中文拒绝提示。

/// 解析 dry-run 的 TransactionValidityError。
pub fn classify_invalid_tx(result_bytes: &[u8]) -> String {
    if result_bytes.len() > 1 && result_bytes[1] == 0x00 {
        let kind = match result_bytes.get(2).copied().unwrap_or(0xff) {
            0 => "Call(当前链状态下不可调度)",
            1 => "Payment(余额不足以支付手续费)",
            2 => "Future(nonce 超前，交易会卡在 future 队列永不出块)",
            3 => "Stale(nonce 已被消费，交易过期)",
            4 => "BadProof(签名校验失败)",
            5 => "AncientBirthBlock(签名时代过旧)",
            6 => "ExhaustsResources(资源超限，请稍后重试)",
            7 => "Custom(自定义交易校验失败)",
            8 => "BadMandatory(强制交易无效)",
            9 => "MandatoryValidation(强制校验失败)",
            10 => "BadSigner(签名账户无效)",
            11 => "IndeterminateImplicit(隐式校验数据不确定)",
            12 => "UnknownOrigin(交易来源未知)",
            _ => "Unknown",
        };
        format!("InvalidTransaction::{kind}")
    } else {
        "UnknownTransaction".to_string()
    }
}

/// 把 dry-run 拒绝结果转为前端报错。
pub fn dry_run_reject_message(result_bytes: &[u8], raw_hex: &str) -> String {
    if result_bytes.starts_with(&[0x01, 0x00, 0x02]) {
        return "上一笔交易尚未出块，请稍候再试".to_string();
    }
    let reason = classify_invalid_tx(result_bytes);
    format!("交易校验失败，已拒绝提交: {reason} (hex: {raw_hex})")
}

/// 把 dry-run RPC 层失败转为中文预检拒绝。
///
/// 这里覆盖 RuntimeApi / wasm trap 等运行时校验崩溃；调用方不得继续
/// `author_submitExtrinsic`，否则同一错误会在交易池终审阶段再次冒出。
pub fn preflight_reject_message(error: &str) -> String {
    if error.contains("RuntimeApi")
        || error.contains("wasm trap")
        || error.contains("unreachable")
        || error.contains("TaggedTransactionQueue_validate_transaction")
    {
        return format!("链运行时校验失败，交易未提交: {error}");
    }
    format!("链交易预检失败，交易未提交: {error}")
}

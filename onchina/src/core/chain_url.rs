//! 链节点连接只使用显式 ONCHAIN_WS_URL 的 WSS 入口；HTTPS 查询从同一地址转换，不回退明文。

pub(crate) fn chain_ws_url() -> Result<String, String> {
    let value = std::env::var("ONCHAIN_WS_URL").map_err(|_| "ONCHAIN_WS_URL 未配置".to_string())?;
    let url =
        reqwest::Url::parse(value.trim()).map_err(|_| "ONCHAIN_WS_URL WSS 地址无效".to_string())?;
    if url.scheme() != "wss"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("ONCHAIN_WS_URL 只允许完整 WSS 地址".to_string());
    }
    Ok(url.to_string())
}

pub(crate) fn chain_http_url() -> Result<String, String> {
    let mut url =
        reqwest::Url::parse(&chain_ws_url()?).map_err(|_| "链 TLS 地址无效".to_string())?;
    url.set_scheme("https")
        .map_err(|_| "链 HTTPS 转换失败".to_string())?;
    Ok(url.to_string())
}

// 中文注释：所有 HTTPS RPC 请求共同拒绝重定向降级，保留系统证书与主机名验证。
pub(crate) fn rpc_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("build TLS RPC client failed: {e}"))
}

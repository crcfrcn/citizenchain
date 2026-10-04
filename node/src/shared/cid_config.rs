//! 节点访问OnChina的严格HTTPS配置，局域网同样验证证书及主机名。
const DEFAULT_ONCHINA_BASE_URL: &str = "https://onchina.local:8964";

fn validate_base_url(value: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| "OnChina地址无效".to_string())?;
    if url.scheme() != "https" || url.host_str().is_none()
        || !url.username().is_empty() || url.password().is_some()
        || url.query().is_some() || url.fragment().is_some()
    {
        return Err("OnChina地址必须是无凭据、查询或片段的HTTPS基地址".to_string());
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

/// 缺省入口固定；显式空值或无效值失败，不回退到另一地址。
pub(crate) fn onchina_base_url() -> Result<String, String> {
    let value = match std::env::var("ONCHINA_BASE_URL") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => DEFAULT_ONCHINA_BASE_URL.to_string(),
        Err(_) => return Err("OnChina地址环境配置无效".to_string()),
    };
    validate_base_url(&value)
}

#[cfg(test)]
#[path = "cid_config_tests.rs"]
mod tests;

//! 链节点连接只使用显式 ONCHAIN_WS_URL 的 WSS 入口；HTTPS 查询从同一地址转换，不回退明文。

use std::sync::Arc;
use subxt::backend::rpc::RpcClient;
use subxt::{OnlineClient, PolkadotConfig};

/// 桌面父进程只交付公开证书DER；生产服务没有此变量，继续验证系统信任链。
fn desktop_rpc_certificate(url: &str) -> Result<Option<Vec<u8>>, String> {
    let value = match std::env::var("ONCHAIN_LOCAL_RPC_CERTIFICATE") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(_) => return Err("本机链 RPC 公开证书配置无效".into()),
    };
    decode_desktop_rpc_certificate(&value, url).map(Some)
}

fn decode_desktop_rpc_certificate(value: &str, url: &str) -> Result<Vec<u8>, String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "本机链 RPC 地址无效".to_string())?;
    if parsed.host_str() != Some("127.0.0.1")
        || !matches!(parsed.scheme(), "https" | "wss")
    {
        return Err("本机链 RPC 证书只允许回环地址".into());
    }
    hex::decode(value).map_err(|_| "本机链 RPC 公开证书无效".into())
}

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

// 中文注释：远端使用系统信任根；桌面本机地址只信任父进程交付的公开证书。
pub(crate) fn rpc_http_client() -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none());
    let certificate = if std::env::var_os("ONCHAIN_LOCAL_RPC_CERTIFICATE").is_some() {
        desktop_rpc_certificate(&chain_http_url()?)?
    } else {
        None
    };
    if let Some(certificate) = certificate {
        let trusted = reqwest::Certificate::from_der(&certificate)
            .map_err(|_| "本机链 RPC 公开证书无效".to_string())?;
        builder = builder.no_proxy().tls_certs_only([trusted]);
    }
    builder
        .build()
        .map_err(|e| format!("build TLS RPC client failed: {e}"))
}

/// 所有链WSS调用共用此入口；桌面连接只信任父进程交付的本次公开证书。
pub(crate) async fn rpc_client(ws_url: &str) -> Result<RpcClient, String> {
    let Some(certificate) = desktop_rpc_certificate(ws_url)? else {
        return RpcClient::from_insecure_url(ws_url)
            .await
            .map_err(|error| format!("连接链 WSS 失败: {error}"));
    };
    let config = trusted_websocket_config(certificate)?;
    let client = subxt::ext::jsonrpsee::ws_client::WsClientBuilder::new()
        .with_custom_cert_store(config)
        .max_redirections(0)
        .build(ws_url)
        .await
        .map_err(|error| format!("连接本机链 WSS 失败: {error}"))?;
    Ok(RpcClient::new(client))
}

fn trusted_websocket_config(certificate: Vec<u8>) -> Result<rustls::ClientConfig, String> {
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(rustls::pki_types::CertificateDer::from(certificate))
        .map_err(|_| "本机链 RPC 公开证书无效".to_string())?;
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
        .with_safe_default_protocol_versions()
        .map_err(|error| format!("创建本机链 WSS 信任配置失败: {error}"))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(config)
}

pub(crate) async fn online_client(ws_url: &str) -> Result<OnlineClient<PolkadotConfig>, String> {
    OnlineClient::<PolkadotConfig>::from_rpc_client(rpc_client(ws_url).await?)
        .await
        .map_err(|error| format!("连接链客户端失败: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{decode_desktop_rpc_certificate, trusted_websocket_config};

    #[test]
    fn desktop_chain_certificate_is_bound_to_loopback_and_valid_hex() {
        assert_eq!(
            decode_desktop_rpc_certificate("010203", "wss://127.0.0.1:9944/").unwrap(),
            vec![1, 2, 3]
        );
        for url in [
            "wss://example.com:9944/",
            "wss://localhost:9944/",
            "ws://127.0.0.1:9944/",
            "not-a-url",
        ] {
            assert!(decode_desktop_rpc_certificate("010203", url).is_err());
        }
        assert!(decode_desktop_rpc_certificate("xyz", "wss://127.0.0.1:9944/").is_err());
        let generated = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        assert!(trusted_websocket_config(generated.cert.der().to_vec()).is_ok());
        assert!(trusted_websocket_config(vec![1, 2, 3]).is_err());
    }
}

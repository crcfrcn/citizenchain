use nodedesktop::constants::RPC_RESPONSE_LIMIT_LARGE;
use nodedesktop::rpc::RPC_REQUEST_TIMEOUT;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "contentType")]
pub enum TabContent {
    RuntimeConstitution,
    Text { text: String },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtherTabItem {
    pub key: String,
    pub title: String,
    #[serde(flatten)]
    pub content: TabContent,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtherTabsPayload {
    pub tabs: Vec<OtherTabItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeConstitutionDocument {
    pub html: String,
    pub blake2_256: String,
    pub source: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WhitepaperDocument {
    pub markdown: String,
}

const WHITEPAPER_CONTENT_URL: &str = "https://www.crcfrcn.com/whitepaper/content.json";

const WHITEPAPER_CONTENT_LIMIT: usize = 1_048_576;

fn decode_whitepaper_document(bytes: &[u8]) -> Result<WhitepaperDocument, String> {
    if bytes.len() > WHITEPAPER_CONTENT_LIMIT {
        return Err("官网白皮书正文超限".to_string());
    }
    let document: WhitepaperDocument =
        serde_json::from_slice(bytes).map_err(|_| "官网白皮书正文格式无效".to_string())?;
    if document.markdown.trim().is_empty() {
        return Err("官网白皮书正文为空".to_string());
    }
    Ok(document)
}

// 节点仅在打开白皮书时读取官网静态正文；HTTPS证书和主机名由系统TLS校验。
#[tauri::command]
pub async fn get_whitepaper_document() -> Result<WhitepaperDocument, String> {
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| "官网白皮书连接初始化失败".to_string())?;
    let mut response = client
        .get(WHITEPAPER_CONTENT_URL)
        .send()
        .await
        .map_err(|_| "官网白皮书读取失败".to_string())?;
    if response.status() != reqwest::StatusCode::OK {
        return Err("官网白皮书读取失败".to_string());
    }
    let mut bytes = Vec::new();
    // 异步分块读取，不阻塞桌面事件处理，且在累积前执行正文大小限制。
    while let Some(part) = response
        .chunk()
        .await
        .map_err(|_| "官网白皮书正文读取失败".to_string())?
    {
        if part.len() > WHITEPAPER_CONTENT_LIMIT - bytes.len() {
            return Err("官网白皮书正文超限".to_string());
        }
        bytes.extend_from_slice(&part);
    }
    decode_whitepaper_document(&bytes)
}

#[tauri::command]
pub fn get_other_tabs_content() -> Result<OtherTabsPayload, String> {
    Ok(OtherTabsPayload {
        tabs: vec![
            OtherTabItem {
                key: "constitution".to_string(),
                title: "公民宪法".to_string(),
                content: crate::documents::TabContent::RuntimeConstitution,
            },
            OtherTabItem {
                key: "party".to_string(),
                title: "公民党".to_string(),
                content: crate::documents::TabContent::Text {
                    text: "更多功能开发中。".to_string(),
                },
            },
        ],
    })
}

#[tauri::command]
pub fn get_runtime_constitution_document() -> Result<RuntimeConstitutionDocument, String> {
    let result = nodedesktop::rpc::rpc_post(
        "constitution_getDocument",
        Value::Array(vec![]),
        RPC_REQUEST_TIMEOUT,
        RPC_RESPONSE_LIMIT_LARGE,
    )?;
    let html = result
        .get("html")
        .and_then(Value::as_str)
        .ok_or_else(|| "runtime 公民宪法响应缺少 html".to_string())?
        .to_string();
    let blake2_256 = result
        .get("blake2_256")
        .and_then(Value::as_str)
        .ok_or_else(|| "runtime 公民宪法响应缺少 blake2_256".to_string())?
        .to_string();
    let source = result
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("runtime")
        .to_string();
    Ok(RuntimeConstitutionDocument {
        html,
        blake2_256,
        source,
    })
}
#[cfg(test)]
#[path = "../tests/documents.rs"]
mod whitepaper_tests;

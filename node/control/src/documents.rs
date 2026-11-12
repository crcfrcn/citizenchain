use nodedesktop::constants::RPC_RESPONSE_LIMIT_LARGE;
use nodedesktop::rpc::RPC_REQUEST_TIMEOUT;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

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

// 唯一正文和配图随节点编译内置，路径相对本文件定位到公民链根目录。
// 返回格式保持不变，前端继续使用原有渲染器、目录及会话缓存。
#[tauri::command]
pub async fn get_whitepaper_document() -> Result<WhitepaperDocument, String> {
    let mut markdown = include_str!("../../../CitizenChainWhitepaper.md").to_string();
    for (name, svg) in [
        (
            "whitepaper-node-diagram.svg",
            include_str!("../../../icons/whitepaper-node-diagram.svg"),
        ),
        (
            "whitepaper-reserve-architecture.svg",
            include_str!("../../../icons/whitepaper-reserve-architecture.svg"),
        ),
    ] {
        // 将原 SVG 字节编码为 data URL，安装后无须访问源码目录或官网。
        let encoded: String = svg.bytes().map(|byte| format!("%{byte:02X}")).collect();
        markdown = markdown.replace(
            &format!("./icons/{name}"),
            &format!("data:image/svg+xml,{encoded}"),
        );
    }
    Ok(WhitepaperDocument { markdown })
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

use serde::Serialize;
use serde_json::Value;

use crate::shared::{
    constants::RPC_RESPONSE_LIMIT_LARGE,
    rpc::{self, RPC_REQUEST_TIMEOUT},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "contentType")]
pub enum TabContent {
    Document,
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

#[tauri::command]
pub fn get_other_tabs_content() -> Result<OtherTabsPayload, String> {
    Ok(OtherTabsPayload {
        tabs: vec![
            OtherTabItem {
                key: "whitepaper".to_string(),
                title: "白皮书".to_string(),
                content: TabContent::Document,
            },
            OtherTabItem {
                key: "constitution".to_string(),
                title: "公民宪法".to_string(),
                content: TabContent::RuntimeConstitution,
            },
            OtherTabItem {
                key: "party".to_string(),
                title: "公民党".to_string(),
                content: TabContent::Text {
                    text: "更多功能开发中。".to_string(),
                },
            },
        ],
    })
}

#[tauri::command]
pub fn get_runtime_constitution_document() -> Result<RuntimeConstitutionDocument, String> {
    let result = rpc::rpc_post(
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

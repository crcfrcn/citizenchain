//! 公民证件资料模型，字段与序列化保持不变。
use chrono::{DateTime, Utc};
use serde::Serialize;
pub(crate) const CITIZEN_DOCUMENT_TYPES: [&str; 4] =
    ["护照相片", "出生证明", "监护人护照", "其他材料"];

/// 公民独立资料库文件元数据。
///
/// 公民资料库必须独立于机构 docs 表;文件本体存磁盘,
/// citizen_documents 只保存当前公民资料文件的元数据和内容哈希。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CitizenDocument {
    pub(crate) id: u64,
    pub(crate) cid_number: String,
    pub(crate) file_name: String,
    pub(crate) document_type: String,
    pub(crate) file_size: u64,
    pub(crate) file_hash: String,
    pub(crate) uploader_account_id: String,
    pub(crate) uploaded_at: DateTime<Utc>,
}

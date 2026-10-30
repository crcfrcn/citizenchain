use crate::types::EligibleClearingBankCandidate;
use serde::Deserialize;
use std::time::Duration;

const ONCHINA_REQUEST_TIMEOUT: Duration = Duration::from_secs(8);

/// OnChina `ApiResponse<Vec<EligibleClearingBankRow>>` 的反序列化形态。
///
/// OnChina 端用统一 `ApiResponse { code, message, data }`,`data` 直接是 Vec(无 items 信封)。
#[derive(Deserialize)]
struct EligibleSearchEnvelope {
    code: Option<u32>,
    #[serde(default)]
    data: Vec<CidEligibleRow>,
    #[serde(default)]
    message: Option<String>,
}

/// OnChina 端原始字段(snake_case)。仅本文件内部用,不对外暴露。
#[derive(Deserialize)]
struct CidEligibleRow {
    cid_number: String,
    #[serde(default)]
    cid_full_name: Option<String>,
    ref_property: String,
    #[serde(default)]
    sub_type: Option<String>,
    #[serde(default)]
    parent_cid_number: Option<String>,
    #[serde(default)]
    parent_full_name: Option<String>,
    #[serde(default)]
    parent_ref_property: Option<String>,
    province_name: String,
    city_name: String,
    #[serde(default)]
    main_account_id: Option<String>,
    #[serde(default)]
    fee_account_id: Option<String>,
}

fn into_candidate(row: CidEligibleRow) -> EligibleClearingBankCandidate {
    EligibleClearingBankCandidate {
        cid_number: row.cid_number,
        cid_full_name: row.cid_full_name.unwrap_or_default(),
        ref_property: row.ref_property,
        sub_type: row.sub_type,
        parent_cid_number: row.parent_cid_number,
        parent_full_name: row.parent_full_name,
        parent_ref_property: row.parent_ref_property,
        province_name: row.province_name,
        city_name: row.city_name,
        main_account_id: row.main_account_id,
        fee_account_id: row.fee_account_id,
    }
}

/// `q` 关键字模糊匹配 cid_number 或机构名。`limit` 上限 50,默认 20。
pub(crate) fn search_eligible_clearing_banks(
    q: &str,
    limit: u32,
) -> Result<Vec<EligibleClearingBankCandidate>, String> {
    let limit = limit.clamp(1, 50);
    let q_trim = q.trim();
    let base_url = nodedesktop::cid::onchina_base_url()?;
    let url = format!("{}/api/app/clearing-banks/eligible-search", base_url);

    let client = onchina_client()?
        .connect_timeout(ONCHINA_REQUEST_TIMEOUT)
        .timeout(ONCHINA_REQUEST_TIMEOUT)
        .build()
        .map_err(|e| format!("创建 OnChina HTTP 客户端失败:{e}"))?;

    let response = client
        .get(&url)
        // reqwest::query 自动按 application/x-www-form-urlencoded 转义 q 中的特殊字符,
        // 避免手动拼接时遇到中文/% 等导致 OnChina 端解析失败。
        .query(&[("q", q_trim), ("limit", &limit.to_string())])
        .send()
        .map_err(|e| format!("OnChina eligible-search 请求失败:{e}"))?;

    if response.status() != reqwest::StatusCode::OK {
        return Err(format!("OnChina 返回 HTTP {}", response.status()));
    }

    let body: EligibleSearchEnvelope = response
        .json()
        .map_err(|e| format!("OnChina 响应解析失败:{e}"))?;

    if body.code != Some(0) {
        let msg = body.message.unwrap_or_default();
        return Err(format!(
            "OnChina 返回错误:code={:?}, message={msg}",
            body.code
        ));
    }

    Ok(body.data.into_iter().map(into_candidate).collect())
}

// 公网使用系统受信根；局域网可显式补充受信CA，始终验证证书和主机名。
fn onchina_client() -> Result<reqwest::blocking::ClientBuilder, String> {
    let mut builder = reqwest::blocking::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none());
    match std::env::var("ONCHINA_TLS_CA_FILE") {
        Ok(path) => {
            if path.is_empty() {
                return Err("OnChina受信CA路径为空".to_string());
            }
            let pem = std::fs::read(path).map_err(|_| "读取OnChina受信CA失败".to_string())?;
            let certificate = reqwest::Certificate::from_pem(&pem)
                .map_err(|_| "OnChina受信CA格式无效".to_string())?;
            builder = builder.add_root_certificate(certificate);
        }
        Err(std::env::VarError::NotPresent) => {}
        Err(_) => return Err("OnChina受信CA配置无效".to_string()),
    }
    Ok(builder)
}

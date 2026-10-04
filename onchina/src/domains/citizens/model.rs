//! 公民电子护照记录与查询 DTO。
//!
//! 公民由注册局先录入本地档案:创建成功即写入身份 CID 与护照号。
//! 链账户只在链上身份推送时绑定，并由该账户签名确认。
//! 本模块不再保留旧绑定态或旧选举范围字段;选举/被选举范围由业务投票规则
//! 结合出生地、居住地行政区计算。

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum CitizenStatus {
    Normal,
    Revoked,
}

/// 公民电子护照记录。
///
/// `account_id` 是链上推送阶段的可选绑定信息；本地新增儿童或未绑定链账户的公民
/// 保持为空。数据库只保存规范化账户 ID，SS58 地址仅在返回展示数据时派生。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CitizenRecord {
    pub(crate) id: u64,
    pub(crate) cid_number: String,
    pub(crate) passport_no: String,
    pub(crate) family_name: String,
    pub(crate) given_name: String,
    pub(crate) citizen_sex: String,
    pub(crate) citizen_birth_date: String,
    #[serde(default)]
    pub(crate) account_id: Option<String>,
    /// finalized 当前绑定版本；0 只允许表示尚未上链的本地档案。
    #[serde(default)]
    pub(crate) binding_revision: u64,
    #[serde(default)]
    pub(crate) binding_finalized_block_number: Option<i64>,
    #[serde(default)]
    pub(crate) binding_finalized_block_hash: Option<String>,
    pub(crate) citizen_status: CitizenStatus,
    #[serde(default)]
    pub(crate) voting_eligible: bool,
    pub(crate) passport_valid_from: String,
    pub(crate) passport_valid_until: String,
    #[serde(default)]
    pub(crate) status_updated_at: Option<i64>,
    pub(crate) province_code: String,
    pub(crate) city_code: String,
    pub(crate) town_code: String,
    pub(crate) birth_province_code: String,
    pub(crate) birth_city_code: String,
    pub(crate) birth_town_code: String,
    #[serde(default)]
    pub(crate) archive_hash: Option<String>,
    #[serde(default)]
    pub(crate) onchain_tx_hash: Option<String>,
    #[serde(default)]
    pub(crate) onchain_block_number: Option<i64>,
    #[serde(default)]
    pub(crate) onchain_at: Option<DateTime<Utc>>,
    pub(crate) creator_account_id: String,
    pub(crate) created_at: DateTime<Utc>,
    #[serde(default)]
    pub(crate) updater_account_id: Option<String>,
    pub(crate) updated_at: DateTime<Utc>,
}

impl CitizenRecord {
    pub(crate) fn computed_identity_status(&self) -> CitizenStatus {
        self.computed_identity_status_on_date(Utc::now().date_naive())
    }

    pub(crate) fn computed_identity_status_on_date(&self, today: NaiveDate) -> CitizenStatus {
        if self.citizen_status != CitizenStatus::Normal {
            return CitizenStatus::Revoked;
        }
        let Some(valid_from) = parse_passport_date(self.passport_valid_from.as_str()) else {
            return CitizenStatus::Revoked;
        };
        let Some(valid_until) = parse_passport_date(self.passport_valid_until.as_str()) else {
            return CitizenStatus::Revoked;
        };
        if valid_from <= today && today <= valid_until {
            CitizenStatus::Normal
        } else {
            CitizenStatus::Revoked
        }
    }

    pub(crate) fn computed_vote_status(&self) -> CitizenStatus {
        if self.voting_eligible && self.computed_identity_status() == CitizenStatus::Normal {
            CitizenStatus::Normal
        } else {
            CitizenStatus::Revoked
        }
    }
}

fn parse_passport_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").ok()
}

#[derive(Deserialize)]
pub(crate) struct CitizensQuery {
    pub(crate) keyword: Option<String>,
    pub(crate) province_name: Option<String>,
    pub(crate) city_name: Option<String>,
    pub(crate) cursor: Option<String>,
    pub(crate) page_size: Option<usize>,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
}

#[derive(Deserialize)]
pub(crate) struct PublicIdentitySearchQuery {
    pub(crate) identity_code: Option<String>,
    pub(crate) account_id: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct PublicIdentitySearchOutput {
    pub(crate) found: bool,
    pub(crate) identity_code: Option<String>,
    pub(crate) account_id: Option<String>,
}

pub(crate) const CITIZEN_DOCUMENT_TYPES: [&str; 4] =
    ["护照相片", "出生证明", "监护人护照", "其他材料"];

/// 公民独立资料库文件元数据。
///
/// 公民资料库必须独立于机构 docs 表;文件本体存磁盘,
/// citizen_documents 只保存当前公民资料文件的元数据和内容哈希。
#[derive(Debug, Clone, Serialize)]
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

#[derive(Serialize)]
pub(crate) struct CitizenRow {
    pub(crate) id: u64,
    pub(crate) cid_number: String,
    pub(crate) passport_no: String,
    pub(crate) family_name: String,
    pub(crate) given_name: String,
    pub(crate) citizen_sex: String,
    pub(crate) citizen_birth_date: String,
    pub(crate) account_id: Option<String>,
    pub(crate) ss58_address: Option<String>,
    pub(crate) binding_revision: u64,
    pub(crate) binding_finalized_block_number: Option<i64>,
    pub(crate) binding_finalized_block_hash: Option<String>,
    pub(crate) citizen_status: CitizenStatus,
    pub(crate) voting_eligible: bool,
    pub(crate) vote_status: CitizenStatus,
    pub(crate) identity_status: CitizenStatus,
    pub(crate) passport_valid_from: String,
    pub(crate) passport_valid_until: String,
    pub(crate) status_updated_at: Option<i64>,
    pub(crate) province_code: String,
    pub(crate) city_code: String,
    pub(crate) town_code: String,
    pub(crate) province_name: Option<String>,
    pub(crate) city_name: Option<String>,
    pub(crate) town_name: Option<String>,
    pub(crate) birth_province_code: String,
    pub(crate) birth_city_code: String,
    pub(crate) birth_town_code: String,
    pub(crate) birth_province_name: Option<String>,
    pub(crate) birth_city_name: Option<String>,
    pub(crate) birth_town_name: Option<String>,
    pub(crate) archive_hash: Option<String>,
    pub(crate) onchain_tx_hash: Option<String>,
    pub(crate) onchain_block_number: Option<i64>,
    pub(crate) onchain_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
// 日期边界夹具均为固定输入，解包失败即代表模型契约回归。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn normal_record() -> CitizenRecord {
        let now = Utc::now();
        CitizenRecord {
            id: 1,
            cid_number: "GD000-CTZN1-2026-TEST".to_string(),
            passport_no: "GD12345678A".to_string(),
            family_name: "测".to_string(),
            given_name: "试公民".to_string(),
            citizen_sex: "FEMALE".to_string(),
            citizen_birth_date: "2000-01-01".to_string(),
            account_id: Some(format!("0x{}", "ab".repeat(32))),
            binding_revision: 1,
            binding_finalized_block_number: Some(7),
            binding_finalized_block_hash: Some(format!("0x{}", "11".repeat(32))),
            citizen_status: CitizenStatus::Normal,
            voting_eligible: true,
            passport_valid_from: "2026-05-24".to_string(),
            passport_valid_until: "2036-05-23".to_string(),
            status_updated_at: Some(1_779_580_800),
            province_code: "GD".to_string(),
            city_code: "001".to_string(),
            town_code: "001001".to_string(),
            birth_province_code: "GD".to_string(),
            birth_city_code: "001".to_string(),
            birth_town_code: "001001".to_string(),
            archive_hash: None,
            onchain_tx_hash: None,
            onchain_block_number: None,
            onchain_at: None,
            creator_account_id: "admin".to_string(),
            created_at: now,
            updater_account_id: None,
            updated_at: now,
        }
    }

    #[test]
    fn citizen_record_computes_identity_status_from_status_and_validity() {
        let record = normal_record();

        assert_eq!(
            record.computed_identity_status_on_date(NaiveDate::from_ymd_opt(2026, 5, 24).unwrap()),
            CitizenStatus::Normal
        );
        assert_eq!(
            record.computed_identity_status_on_date(NaiveDate::from_ymd_opt(2036, 5, 23).unwrap()),
            CitizenStatus::Normal
        );
        assert_eq!(
            record.computed_identity_status_on_date(NaiveDate::from_ymd_opt(2036, 5, 24).unwrap()),
            CitizenStatus::Revoked
        );
        assert_eq!(record.computed_vote_status(), CitizenStatus::Normal);
    }
}

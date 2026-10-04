// 治理机构注册表：直接读取 runtime 常量，避免 node 侧再维护过期地址副本。

use primitives::cid::china::china_cb::{ChinaCb, CHINA_CB, SAFETY_FUND_ACCOUNT};
use primitives::cid::china::china_ch::{ChinaCh, CHINA_CH};

use super::types::{GovernanceOverview, InstitutionListItem, InstitutionType};

#[derive(Clone, Copy)]
pub(crate) enum InstitutionRef {
    Nrc(&'static ChinaCb),
    Prc(&'static ChinaCb),
    Prb(&'static ChinaCh),
}

impl InstitutionRef {
    pub(crate) fn cid_full_name(self) -> &'static str {
        match self {
            InstitutionRef::Nrc(item) | InstitutionRef::Prc(item) => item.cid_full_name,
            InstitutionRef::Prb(item) => item.cid_full_name,
        }
    }

    pub(crate) fn cid_short_name(self) -> &'static str {
        match self {
            InstitutionRef::Nrc(item) | InstitutionRef::Prc(item) => item.cid_short_name,
            InstitutionRef::Prb(item) => item.cid_short_name,
        }
    }

    pub(crate) fn cid_full_name_en(self) -> &'static str {
        match self {
            InstitutionRef::Nrc(item) | InstitutionRef::Prc(item) => item.cid_full_name_en,
            InstitutionRef::Prb(item) => item.cid_full_name_en,
        }
    }

    pub(crate) fn cid_short_name_en(self) -> &'static str {
        match self {
            InstitutionRef::Nrc(item) | InstitutionRef::Prc(item) => item.cid_short_name_en,
            InstitutionRef::Prb(item) => item.cid_short_name_en,
        }
    }

    pub(crate) fn cid_number(self) -> &'static str {
        match self {
            InstitutionRef::Nrc(item) | InstitutionRef::Prc(item) => item.cid_number,
            InstitutionRef::Prb(item) => item.cid_number,
        }
    }

    pub(crate) fn org_type(self) -> InstitutionType {
        match self {
            InstitutionRef::Nrc(_) => InstitutionType::Nrc,
            InstitutionRef::Prc(_) => InstitutionType::Prc,
            InstitutionRef::Prb(_) => InstitutionType::Prb,
        }
    }

    pub(crate) fn main_account_id(self) -> String {
        match self {
            InstitutionRef::Nrc(item) | InstitutionRef::Prc(item) => {
                format!("0x{}", hex::encode(item.main_account))
            }
            InstitutionRef::Prb(item) => format!("0x{}", hex::encode(item.main_account)),
        }
    }

    pub(crate) fn fee_account_id(self) -> String {
        match self {
            InstitutionRef::Nrc(item) | InstitutionRef::Prc(item) => {
                format!("0x{}", hex::encode(item.fee_account))
            }
            InstitutionRef::Prb(item) => format!("0x{}", hex::encode(item.fee_account)),
        }
    }

    pub(crate) fn stake_account_id(self) -> Option<String> {
        match self {
            InstitutionRef::Prb(item) => Some(format!("0x{}", hex::encode(item.stake_account))),
            InstitutionRef::Nrc(_) | InstitutionRef::Prc(_) => None,
        }
    }

    pub(crate) fn safety_fund_account_id(self) -> Option<String> {
        match self {
            InstitutionRef::Nrc(_) => Some(format!("0x{}", hex::encode(SAFETY_FUND_ACCOUNT))),
            InstitutionRef::Prc(_) | InstitutionRef::Prb(_) => None,
        }
    }

    pub(crate) fn to_list_item(self) -> InstitutionListItem {
        let org_type = self.org_type();
        InstitutionListItem {
            cid_full_name: self.cid_full_name().to_string(),
            cid_short_name: self.cid_short_name().to_string(),
            cid_full_name_en: self.cid_full_name_en().to_string(),
            cid_short_name_en: self.cid_short_name_en().to_string(),
            cid_number: self.cid_number().to_string(),
            org_type: org_type as u8,
            org_type_label: org_type.label().to_string(),
            main_account_id: self.main_account_id(),
        }
    }
}

pub(crate) fn governance_overview() -> GovernanceOverview {
    GovernanceOverview {
        national_councils: CHINA_CB
            .first()
            .map(|item| InstitutionRef::Nrc(item).to_list_item())
            .into_iter()
            .collect(),
        provincial_councils: CHINA_CB
            .iter()
            .skip(1)
            .map(|item| InstitutionRef::Prc(item).to_list_item())
            .collect(),
        provincial_banks: CHINA_CH
            .iter()
            .map(|item| InstitutionRef::Prb(item).to_list_item())
            .collect(),
        warning: None,
    }
}

pub(crate) fn find_institution(cid_number: &str) -> Option<InstitutionRef> {
    if let Some(index) = CHINA_CB
        .iter()
        .position(|item| item.cid_number == cid_number)
    {
        return Some(if index == 0 {
            InstitutionRef::Nrc(&CHINA_CB[index])
        } else {
            InstitutionRef::Prc(&CHINA_CB[index])
        });
    }

    CHINA_CH
        .iter()
        .find(|item| item.cid_number == cid_number)
        .map(InstitutionRef::Prb)
}

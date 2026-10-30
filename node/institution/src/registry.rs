use crate::council::GovernanceOverview;
use crate::council::InstitutionListItem;
use crate::council::InstitutionType;
use primitives::cid::china::china_cb::ChinaCb;
use primitives::cid::china::china_cb::CHINA_CB;
use primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT;
use primitives::cid::china::china_ch::ChinaCh;
use primitives::cid::china::china_ch::CHINA_CH;

#[derive(Clone, Copy)]
pub enum InstitutionRef {
    Nrc(&'static ChinaCb),
    Prc(&'static ChinaCb),
    Prb(&'static ChinaCh),
}

impl InstitutionRef {
    pub fn cid_full_name(self) -> &'static str {
        match self {
            crate::registry::InstitutionRef::Nrc(item)
            | crate::registry::InstitutionRef::Prc(item) => item.cid_full_name,
            crate::registry::InstitutionRef::Prb(item) => item.cid_full_name,
        }
    }

    pub(crate) fn cid_short_name(self) -> &'static str {
        match self {
            crate::registry::InstitutionRef::Nrc(item)
            | crate::registry::InstitutionRef::Prc(item) => item.cid_short_name,
            crate::registry::InstitutionRef::Prb(item) => item.cid_short_name,
        }
    }

    pub(crate) fn full_name_en(self) -> &'static str {
        match self {
            crate::registry::InstitutionRef::Nrc(item)
            | crate::registry::InstitutionRef::Prc(item) => item.full_name_en,
            crate::registry::InstitutionRef::Prb(item) => item.full_name_en,
        }
    }

    pub(crate) fn short_name_en(self) -> &'static str {
        match self {
            crate::registry::InstitutionRef::Nrc(item)
            | crate::registry::InstitutionRef::Prc(item) => item.short_name_en,
            crate::registry::InstitutionRef::Prb(item) => item.short_name_en,
        }
    }

    pub(crate) fn cid_number(self) -> &'static str {
        match self {
            crate::registry::InstitutionRef::Nrc(item)
            | crate::registry::InstitutionRef::Prc(item) => item.cid_number,
            crate::registry::InstitutionRef::Prb(item) => item.cid_number,
        }
    }

    pub(crate) fn org_type(self) -> InstitutionType {
        match self {
            crate::registry::InstitutionRef::Nrc(_) => crate::council::InstitutionType::Nrc,
            crate::registry::InstitutionRef::Prc(_) => crate::council::InstitutionType::Prc,
            crate::registry::InstitutionRef::Prb(_) => crate::council::InstitutionType::Prb,
        }
    }

    pub(crate) fn main_account_id(self) -> String {
        match self {
            crate::registry::InstitutionRef::Nrc(item)
            | crate::registry::InstitutionRef::Prc(item) => {
                format!("0x{}", hex::encode(item.main_account))
            }
            crate::registry::InstitutionRef::Prb(item) => {
                format!("0x{}", hex::encode(item.main_account))
            }
        }
    }

    pub(crate) fn fee_account_id(self) -> String {
        match self {
            crate::registry::InstitutionRef::Nrc(item)
            | crate::registry::InstitutionRef::Prc(item) => {
                format!("0x{}", hex::encode(item.fee_account))
            }
            crate::registry::InstitutionRef::Prb(item) => {
                format!("0x{}", hex::encode(item.fee_account))
            }
        }
    }

    pub(crate) fn stake_account_id(self) -> Option<String> {
        match self {
            crate::registry::InstitutionRef::Prb(item) => {
                Some(format!("0x{}", hex::encode(item.stake_account)))
            }
            crate::registry::InstitutionRef::Nrc(_) | crate::registry::InstitutionRef::Prc(_) => {
                None
            }
        }
    }

    pub(crate) fn fund_account_id(self) -> Option<String> {
        match self {
            crate::registry::InstitutionRef::Nrc(_) => {
                Some(format!("0x{}", hex::encode(SAFETY_FUND_ACCOUNT)))
            }
            crate::registry::InstitutionRef::Prc(_) | crate::registry::InstitutionRef::Prb(_) => {
                None
            }
        }
    }

    pub(crate) fn to_list_item(self) -> InstitutionListItem {
        let org_type = self.org_type();
        InstitutionListItem {
            cid_full_name: self.cid_full_name().to_string(),
            cid_short_name: self.cid_short_name().to_string(),
            full_name_en: self.full_name_en().to_string(),
            short_name_en: self.short_name_en().to_string(),
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
            .map(|item| crate::registry::InstitutionRef::Nrc(item).to_list_item())
            .into_iter()
            .collect(),
        provincial_councils: CHINA_CB
            .iter()
            .skip(1)
            .map(|item| crate::registry::InstitutionRef::Prc(item).to_list_item())
            .collect(),
        provincial_banks: CHINA_CH
            .iter()
            .map(|item| crate::registry::InstitutionRef::Prb(item).to_list_item())
            .collect(),
        warning: None,
    }
}

pub fn find_institution(cid_number: &str) -> Option<InstitutionRef> {
    if let Some(index) = CHINA_CB
        .iter()
        .position(|item| item.cid_number == cid_number)
    {
        return Some(if index == 0 {
            crate::registry::InstitutionRef::Nrc(&CHINA_CB[index])
        } else {
            crate::registry::InstitutionRef::Prc(&CHINA_CB[index])
        });
    }

    CHINA_CH
        .iter()
        .find(|item| item.cid_number == cid_number)
        .map(crate::registry::InstitutionRef::Prb)
}

#[cfg(test)]
#[path = "../tests/integration.rs"]
mod integration_tests;

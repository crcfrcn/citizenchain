//! 定义各类私权机构可引用的参与人角色。

use serde::{Deserialize, Serialize};

/// 私权机构参与人角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum ParticipantRole {
    ResponsiblePerson,
    GeneralPartner,
    LimitedPartner,
    EquityShareholder,
    Shareholder,
    Promoter,
    Member,
    Director,
    Supervisor,
}

impl ParticipantRole {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::ResponsiblePerson => "负责人",
            Self::GeneralPartner => "普通合伙人",
            Self::LimitedPartner => "有限合伙人",
            Self::EquityShareholder => "股东",
            Self::Shareholder => "股份股东",
            Self::Promoter => "发起人",
            Self::Member => "成员",
            Self::Director => "理事",
            Self::Supervisor => "监事",
        }
    }
}

pub(crate) const SOLE_ROLES: &[ParticipantRole] = &[ParticipantRole::ResponsiblePerson];

pub(crate) const PARTNERSHIP_ROLES: &[ParticipantRole] = &[
    ParticipantRole::GeneralPartner,
    ParticipantRole::LimitedPartner,
];

pub(crate) const COMPANY_ROLES: &[ParticipantRole] = &[
    ParticipantRole::EquityShareholder,
    ParticipantRole::Director,
    ParticipantRole::Supervisor,
];

pub(crate) const CORPORATION_ROLES: &[ParticipantRole] = &[
    ParticipantRole::Promoter,
    ParticipantRole::Shareholder,
    ParticipantRole::Director,
    ParticipantRole::Supervisor,
];

pub(crate) const WELFARE_ROLES: &[ParticipantRole] = &[
    ParticipantRole::Member,
    ParticipantRole::Director,
    ParticipantRole::Supervisor,
];

pub(crate) const ASSOCIATION_ROLES: &[ParticipantRole] = &[
    ParticipantRole::Member,
    ParticipantRole::Director,
    ParticipantRole::Supervisor,
];

//! 机构/账户业务校验 + 分类 + 唯一性
//!
//! 凡是要在 handler 里做的业务级校验(不是简单的格式校验),都放这里。
//! handler 只负责调用 service + 转 HTTP 响应。

use crate::cid::code;
use crate::cid::{validate_cid_number_format, AdminLevel};
use crate::institution::subjects::model::Institution;
use primitives::account_derive::is_forbidden_account_name;

// 保留名字面单源 = primitives::account_derive::RESERVED_NAME_*_STR。
pub const ACCOUNT_NAME_MAIN: &str = primitives::account_derive::RESERVED_NAME_MAIN_STR;

pub const MAX_ACCOUNT_NAME_CHARS: usize = 30;
pub const MAX_ACCOUNT_NAME_BYTES: usize = 128;
pub const MAX_INSTITUTION_NAME_CHARS: usize = 30;
pub const MAX_INSTITUTION_NAME_BYTES: usize = 128;
pub const MAX_PERSON_NAME_CHARS: usize = 30;
pub const MAX_PERSON_NAME_BYTES: usize = 128;
pub const MAX_LEGAL_REP_PHOTO_BYTES: u64 = 5 * 1024 * 1024;

pub struct LegalRepresentativeFields {
    pub family_name: String,
    pub given_name: String,
    pub cid_number: String,
    pub photo_path: String,
    pub photo_name: String,
    pub photo_mime: String,
    pub photo_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegalRepresentativeCitizenScope {
    Nationwide,
    Province {
        province_code: String,
    },
    City {
        province_code: String,
        city_code: String,
    },
}

impl LegalRepresentativeCitizenScope {
    pub fn province_code(&self) -> Option<&str> {
        match self {
            Self::Nationwide => None,
            Self::Province { province_code } | Self::City { province_code, .. } => {
                Some(province_code.as_str())
            }
        }
    }

    pub fn city_code(&self) -> Option<&str> {
        match self {
            Self::City { city_code, .. } => Some(city_code.as_str()),
            Self::Nationwide | Self::Province { .. } => None,
        }
    }

    pub fn legal_representative_error_message(&self) -> &'static str {
        match self {
            Self::Nationwide => "法定代表人身份ID必须选择正常状态公民",
            Self::Province { .. } => "该机构法定代表人必须是本省正常状态公民",
            Self::City { .. } => "该机构法定代表人必须是本市正常状态公民",
        }
    }
}

fn local_public_scope(province_code: &str, city_code: &str) -> LegalRepresentativeCitizenScope {
    let province_code = province_code.trim().to_string();
    let city_code = city_code.trim().to_string();
    if city_code.is_empty() || city_code == "000" {
        LegalRepresentativeCitizenScope::Province { province_code }
    } else {
        LegalRepresentativeCitizenScope::City {
            province_code,
            city_code,
        }
    }
}

fn public_org_scope(
    institution_code: &str,
    province_code: &str,
    city_code: &str,
) -> LegalRepresentativeCitizenScope {
    match code::institution_code_from_str(institution_code).and_then(|c| code::admin_level(&c)) {
        Some(AdminLevel::National) => LegalRepresentativeCitizenScope::Nationwide,
        Some(AdminLevel::Province) => LegalRepresentativeCitizenScope::Province {
            province_code: province_code.trim().to_string(),
        },
        // 市级、镇级、无层级公权机构都按落位省市收口;若没有市码则按省级处理。
        Some(AdminLevel::City) | Some(AdminLevel::Town) | None => {
            local_public_scope(province_code, city_code)
        }
    }
}

pub fn resolve_legal_representative_scope_for_codes(
    institution_code: &str,
    _education_type: Option<&str>,
    province_code: &str,
    city_code: &str,
    parent: Option<&Institution>,
) -> LegalRepresentativeCitizenScope {
    let parsed_institution_code = code::institution_code_from_str(institution_code);
    let is_public_legal = parsed_institution_code.is_some_and(|c| code::is_public_legal_code(&c));
    let is_unincorporated =
        parsed_institution_code.is_some_and(|c| code::is_unincorporated_code(&c));
    if is_public_legal {
        return public_org_scope(institution_code, province_code, city_code);
    }

    let parent_is_public_legal_person = parent
        .map(|parent| {
            code::institution_code_from_str(parent.institution_code.as_str())
                .is_some_and(|c| code::is_public_legal_code(&c))
        })
        .unwrap_or(false);
    if is_unincorporated && parent_is_public_legal_person {
        return local_public_scope(province_code, city_code);
    }

    // 私法人、私法人学校、挂靠私法人的非法人学校/机构都允许全国正常公民担任法定代表人。
    LegalRepresentativeCitizenScope::Nationwide
}

pub fn resolve_legal_representative_scope_for_institution(
    inst: &Institution,
    parent: Option<&Institution>,
) -> LegalRepresentativeCitizenScope {
    resolve_legal_representative_scope_for_codes(
        inst.institution_code.as_str(),
        inst.education_type.as_deref(),
        inst.province_code.as_str(),
        inst.city_code.as_str(),
        parent,
    )
}

pub fn is_protocol_account_name(account_name: &str) -> bool {
    primitives::account_derive::institution_protocol_kind_by_name(account_name.as_bytes()).is_some()
}

/// 机构账户分类标签，与 Node/CitizenApp/CitizenWallet 的展示协议一致。
pub fn institution_account_kind_label(
    cid_number: &str,
    account_name: &str,
) -> Option<&'static str> {
    let kind = primitives::account_derive::institution_kind_by_name(
        cid_number.as_bytes(),
        account_name.as_bytes(),
    )?;
    Some(match kind.institution_protocol_kind() {
        Some(primitives::account_derive::InstitutionProtocolAccountKind::Main) => "main",
        Some(primitives::account_derive::InstitutionProtocolAccountKind::Fee) => "fee",
        Some(primitives::account_derive::InstitutionProtocolAccountKind::Stake) => "stake",
        Some(primitives::account_derive::InstitutionProtocolAccountKind::SafetyFund) => {
            "safety_fund"
        }
        Some(primitives::account_derive::InstitutionProtocolAccountKind::He) => "he",
        Some(primitives::account_derive::InstitutionProtocolAccountKind::Clearing) => "clearing",
        Some(
            primitives::account_derive::InstitutionProtocolAccountKind::FederalCitizenSecurityFund,
        ) => "federal_citizen_security_fund",
        None => "named",
    })
}

/// 机构 / 账户 service 层错误。
#[derive(Debug, Clone)]
pub enum ServiceError {
    BadInput(&'static str),
    // NotFound/Conflict 是 service_error_to_response 的 HTTP 映射完备性槽位(404/409);
    // 当前 service 函数只产 BadInput,但映射函数按语义列举全部变体,删变体会破坏映射。
    #[allow(dead_code)]
    NotFound(&'static str),
    #[allow(dead_code)]
    Conflict(&'static str),
}

impl ServiceError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::BadInput(m) | Self::NotFound(m) | Self::Conflict(m) => m,
        }
    }
}

/// 校验机构全称格式。
pub fn validate_cid_full_name(cid_full_name: &str) -> Result<String, ServiceError> {
    let trimmed = cid_full_name.trim();
    if trimmed.is_empty() {
        return Err(ServiceError::BadInput("cid_full_name is required"));
    }
    if trimmed.chars().count() > MAX_INSTITUTION_NAME_CHARS {
        return Err(ServiceError::BadInput(
            "cid_full_name too long (max 30 chars)",
        ));
    }
    if trimmed.len() > MAX_INSTITUTION_NAME_BYTES {
        return Err(ServiceError::BadInput(
            "cid_full_name too long (max 128 bytes)",
        ));
    }
    Ok(trimmed.to_string())
}

pub fn validate_legal_representative_required(
    family_name: Option<&str>,
    given_name: Option<&str>,
    cid_number: Option<&str>,
    photo_path: Option<&str>,
    photo_name: Option<&str>,
    photo_mime: Option<&str>,
    photo_size: Option<u64>,
) -> Result<LegalRepresentativeFields, ServiceError> {
    let family_name = family_name
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or(ServiceError::BadInput("法定代表人姓不能为空"))?;
    if family_name.chars().count() > MAX_PERSON_NAME_CHARS
        || family_name.len() > MAX_PERSON_NAME_BYTES
    {
        return Err(ServiceError::BadInput("法定代表人姓过长"));
    }
    let given_name = given_name
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or(ServiceError::BadInput("法定代表人名不能为空"))?;
    if given_name.chars().count() > MAX_PERSON_NAME_CHARS
        || given_name.len() > MAX_PERSON_NAME_BYTES
    {
        return Err(ServiceError::BadInput("法定代表人名过长"));
    }
    let cid_number = cid_number
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or(ServiceError::BadInput("法定代表人身份ID不能为空"))?;
    let cid_number = validate_cid_number_format(cid_number)
        .map_err(|_| ServiceError::BadInput("法定代表人身份ID格式错误"))?;
    let photo_path = photo_path
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or(ServiceError::BadInput("法定代表人证件照不能为空"))?;
    if !super::photos::valid_photo_path(photo_path) {
        return Err(ServiceError::BadInput("请重新上传法定代表人证件照"));
    }
    let photo_name = photo_name
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or(ServiceError::BadInput("法定代表人证件照文件名不能为空"))?;
    let photo_mime = photo_mime
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or(ServiceError::BadInput("法定代表人证件照类型不能为空"))?;
    if !matches!(photo_mime, "image/jpeg" | "image/png" | "image/webp") {
        return Err(ServiceError::BadInput(
            "法定代表人证件照只支持 JPEG/PNG/WebP",
        ));
    }
    let photo_size = photo_size
        .filter(|v| *v > 0 && *v <= MAX_LEGAL_REP_PHOTO_BYTES)
        .ok_or(ServiceError::BadInput("法定代表人证件照大小非法"))?;
    Ok(LegalRepresentativeFields {
        family_name: family_name.to_string(),
        given_name: given_name.to_string(),
        cid_number,
        photo_path: photo_path.to_string(),
        photo_name: photo_name.to_string(),
        photo_mime: photo_mime.to_string(),
        photo_size,
    })
}

/// 校验账户名称格式。
pub fn validate_account_name(name: &str) -> Result<String, ServiceError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ServiceError::BadInput("account_name is required"));
    }
    if trimmed.chars().count() > MAX_ACCOUNT_NAME_CHARS {
        return Err(ServiceError::BadInput(
            "account_name too long (max 30 chars)",
        ));
    }
    if trimmed.len() > MAX_ACCOUNT_NAME_BYTES {
        return Err(ServiceError::BadInput(
            "account_name too long (max 128 bytes)",
        ));
    }
    // 制度专属保留名(永久质押/安全基金/两和基金)禁止注册为自定义账户名,
    // 与链端 primitives::account_derive::is_forbidden_account_name 单一权威源一致。
    if is_forbidden_account_name(trimmed.as_bytes()) {
        return Err(ServiceError::BadInput(
            "account_name 命中制度专属保留名(永久质押/安全基金/两和基金)",
        ));
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_account_name_basic() {
        assert!(validate_account_name("").is_err());
        assert!(validate_account_name("   ").is_err());
        assert!(validate_account_name("办案账户").is_ok());
        let too_long = "x".repeat(31);
        assert!(validate_account_name(&too_long).is_err());
    }
}

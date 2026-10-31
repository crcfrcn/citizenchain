//! 法律案提案组织:HTTP 请求 + 本节点机构 + 宪法路由 + 链上账户 → 裸 SCALE call-data。
//!
//! onchina 只「组织数据 + 调编码器」,不计票、不提交。发起机构直接使用登录绑定的
//! 现有 CID；其他路由机构由调用方注入的唯一查询解析，保持本层纯函数可测。
//! 合法性最终裁决在链端 `ensure_routing`;本层派生同一 LawRegion，授权前置检查仍按原层级与机构 CID 执行。
//!

use super::chain_propose::{
    encode_propose_amend_law, encode_propose_enact_law, encode_propose_repeal_law,
};
use super::chain_vote::encode_cast_representative_vote;
use super::model::{to_chapter_args, LawActionInput, ProposeLawInput};
use super::routing::{routing_for, vote_type_is_education};
use onchina::institutions_call::ChainCall;

/// 法律案组织错误(fail-closed:任一不满足即拒,不退化)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegislationError {
    /// 该层级×类型无合法宪法路由(如省教育案)。
    UnknownRouting,
    /// 路由机构无法解析唯一 CID(本节点尚未对账 / 机构未上链)。
    InstitutionCidUnresolved,
    /// 立法/修法标题为空。
    EmptyTitle,
    /// 立法/修法正文为空。
    EmptyChapters,
    /// 修法/废法缺 law_id。
    MissingLawId,
    /// 发起岗位码为空或超过链上岗位码上限。
    InvalidProposerRoleCode,
    /// 机构 CID 不能派生合法法律地域。
    ScopeMismatch,
}

impl LegislationError {
    /// 稳定错误码文本(供 HTTP 层映射,handler 接入时使用)。
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownRouting => "LEGISLATION_UNKNOWN_ROUTING",
            Self::InstitutionCidUnresolved => "LEGISLATION_INSTITUTION_CID_UNRESOLVED",
            Self::EmptyTitle => "LEGISLATION_EMPTY_TITLE",
            Self::EmptyChapters => "LEGISLATION_EMPTY_CHAPTERS",
            Self::MissingLawId => "LEGISLATION_MISSING_LAW_ID",
            Self::InvalidProposerRoleCode => "LEGISLATION_INVALID_PROPOSER_ROLE_CODE",
            Self::ScopeMismatch => "LEGISLATION_SCOPE_MISMATCH",
        }
    }
}

/// 校验现有机构 CID 与预期机构码一致，保留原 CID 字节。
fn institution_cid_number(code: [u8; 4], cid_number: &str) -> Result<Vec<u8>, LegislationError> {
    if cid_number.is_empty()
        || cid_number.len() > primitives::core_const::CID_NUMBER_MAX_BYTES as usize
        || cid_number.trim() != cid_number
        || !primitives::cid::number::parse_cid_number_parts_bytes(cid_number.as_bytes())
            .is_ok_and(|parts| parts.institution == code)
    {
        return Err(LegislationError::InstitutionCidUnresolved);
    }
    Ok(cid_number.as_bytes().to_vec())
}

/// 其他路由机构仍经原唯一查询入口取得现有 CID。
fn resolve_institution_cid_number(
    code: [u8; 4],
    resolve_cid_number: &impl Fn(&[u8; 4]) -> Option<String>,
) -> Result<Vec<u8>, LegislationError> {
    let cid_number = resolve_cid_number(&code).ok_or(LegislationError::InstitutionCidUnresolved)?;
    institution_cid_number(code, &cid_number)
}

/// 组织一次法律案发起 → 裸 SCALE call-data。
///
/// `proposer_code` = 本节点绑定机构码(发起院 / 教委会 / 自治会);签名人(origin)= 管理员本人
/// 在冷签层提供。`actor_cid_number` 是登录绑定的原 CID，前端不传；
/// 同一机构作为表决院时复用该 CID，其他路由机构继续唯一查询。
pub fn build_propose_law_call(
    input: &ProposeLawInput,
    proposer_code: [u8; 4],
    actor_cid_number: &str,
    resolve_cid_number: impl Fn(&[u8; 4]) -> Option<String>,
) -> Result<ChainCall, LegislationError> {
    let routing = routing_for(input.tier, vote_type_is_education(input.vote_type))
        .ok_or(LegislationError::UnknownRouting)?;
    let proposer_role_code = input.proposer_role_code.trim().as_bytes();
    if proposer_role_code.is_empty()
        || proposer_role_code.len() > entitybase::INSTITUTION_ROLE_CODE_MAX_BYTES as usize
    {
        return Err(LegislationError::InvalidProposerRoleCode);
    }

    let actor_cid_number = institution_cid_number(proposer_code, actor_cid_number)?;
    let mut houses = Vec::with_capacity(routing.houses.len());
    for code in &routing.houses {
        houses.push(if *code == proposer_code {
            actor_cid_number.clone()
        } else {
            resolve_institution_cid_number(*code, &resolve_cid_number)?
        });
    }
    let executive_cid_number =
        resolve_institution_cid_number(routing.executive, &resolve_cid_number)?;
    let legislature = match routing.legislature {
        Some(code) => Some(resolve_institution_cid_number(code, &resolve_cid_number)?),
        None => None,
    };
    let legislature_ref = legislature.as_ref();

    match input.law_action {
        LawActionInput::Enact => {
            ensure_title_and_chapters(input)?;
            let chapters = to_chapter_args(&input.chapters);
            Ok(encode_propose_enact_law(
                input.tier,
                primitives::genesis::LawRegion::from_cid(input.tier, &actor_cid_number)
                    .map_err(|_| LegislationError::ScopeMismatch)?,
                &houses,
                &actor_cid_number,
                proposer_role_code,
                &executive_cid_number,
                legislature_ref.map(Vec::as_slice),
                input.vote_type,
                input.title.as_bytes(),
                input.title_en.as_deref().map(str::as_bytes),
                &chapters,
                input.effective_at,
            ))
        }
        LawActionInput::Amend => {
            let law_id = input.law_id.ok_or(LegislationError::MissingLawId)?;
            ensure_title_and_chapters(input)?;
            let chapters = to_chapter_args(&input.chapters);
            Ok(encode_propose_amend_law(
                law_id,
                &actor_cid_number,
                proposer_role_code,
                &executive_cid_number,
                legislature_ref.map(Vec::as_slice),
                input.vote_type,
                input.title.as_bytes(),
                input.title_en.as_deref().map(str::as_bytes),
                &chapters,
                input.effective_at,
            ))
        }
        LawActionInput::Repeal => {
            let law_id = input.law_id.ok_or(LegislationError::MissingLawId)?;
            Ok(encode_propose_repeal_law(
                law_id,
                &actor_cid_number,
                proposer_role_code,
                &executive_cid_number,
                legislature_ref.map(Vec::as_slice),
                input.vote_type,
            ))
        }
    }
}

/// 立法/修法必须有非空标题与正文。
fn ensure_title_and_chapters(input: &ProposeLawInput) -> Result<(), LegislationError> {
    if input.title.trim().is_empty() {
        return Err(LegislationError::EmptyTitle);
    }
    if input.chapters.is_empty() {
        return Err(LegislationError::EmptyChapters);
    }
    Ok(())
}

/// 组织一次代表机构表决 → 裸 SCALE call-data。
pub fn build_representative_vote_call(
    proposal_id: u64,
    voter_role_code: &str,
    approve: bool,
) -> ChainCall {
    encode_cast_representative_vote(proposal_id, voter_role_code, approve)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/laws_service.rs"]
mod tests;

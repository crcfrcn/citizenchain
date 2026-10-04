//! 法律案提案组织:HTTP 请求 + 本节点机构 + 宪法路由 + 链上账户 → 裸 SCALE call-data。
//!
//! onchina 只「组织数据 + 调编码器」,不计票、不提交。各机构 CID 由调用方
//! 注入(`resolve_cid_number` 闭包:生产为链读,单测为夹具),保持本层纯函数可测。
//! 合法性最终裁决在链端 `ensure_routing`;本层 + `precheck_legislation_scope` 只做越权前置拦截(fail-closed)。
//!

use super::chain_propose::{
    encode_propose_amend_law, encode_propose_enact_law, encode_propose_repeal_law,
};
use super::chain_vote::encode_cast_representative_vote;
use super::model::{to_chapter_args, LawActionInput, ProposeLawInput};
use super::routing::{routing_for, vote_type_is_education};
use crate::core::institution_call::ChainCall;

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
    /// 提案层级超出本管理员管辖(越权)。
    TierNotAllowedForAdmin,
    /// 提案行政区与本管理员 scope 不一致(越权)。
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
            Self::TierNotAllowedForAdmin => "LEGISLATION_TIER_NOT_ALLOWED",
            Self::ScopeMismatch => "LEGISLATION_SCOPE_MISMATCH",
        }
    }
}

/// 把机构码经唯一查询入口解析为机构 CID。
fn resolve_institution_cid_number(
    code: [u8; 4],
    resolve_cid_number: &impl Fn(&[u8; 4]) -> Option<String>,
) -> Result<Vec<u8>, LegislationError> {
    let cid_number = resolve_cid_number(&code)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= primitives::core_const::CID_NUMBER_MAX_BYTES as usize
        })
        .ok_or(LegislationError::InstitutionCidUnresolved)?;
    Ok(cid_number.into_bytes())
}

/// 写入边界 scope 前置校验(fail-closed)。
///
/// `admin_tier` = 管理员层级(1 国家 / 2 省 / 3 市);`admin_scope_code` = 管理员所辖
/// 行政区码(国家 = 0)。规则:国家管理员可发起国家法律(tier 1)与修宪(tier 0);省/市只能发起本级;
/// 提案行政区码必须等于管理员 scope 码(国家两端均 0)。
pub fn precheck_legislation_scope(
    admin_tier: u8,
    admin_scope_code: u32,
    proposal_tier: u8,
    proposal_scope_code: u32,
) -> Result<(), LegislationError> {
    let tier_allowed = match admin_tier {
        1 => proposal_tier == 1 || proposal_tier == 0, // 国家级:国家法律 + 修宪
        2 => proposal_tier == 2,
        3 => proposal_tier == 3,
        _ => false,
    };
    if !tier_allowed {
        return Err(LegislationError::TierNotAllowedForAdmin);
    }
    if proposal_scope_code != admin_scope_code {
        return Err(LegislationError::ScopeMismatch);
    }
    Ok(())
}

/// 组织一次法律案发起 → 裸 SCALE call-data。
///
/// `proposer_code` = 本节点绑定机构码(发起院 / 教委会 / 自治会);签名人(origin)= 管理员本人
/// 在冷签层提供。所有路由机构只解析为 CID,前端不传(防越权)。
pub fn build_propose_law_call(
    input: &ProposeLawInput,
    proposer_code: [u8; 4],
    resolve_cid_number: impl Fn(&[u8; 4]) -> Option<String>,
) -> Result<ChainCall, LegislationError> {
    let routing = routing_for(input.tier, vote_type_is_education(input.vote_type))
        .ok_or(LegislationError::UnknownRouting)?;
    let proposer_role_code = input.proposer_role_code.trim().as_bytes();
    if proposer_role_code.is_empty()
        || proposer_role_code.len() > entity_primitives::INSTITUTION_ROLE_CODE_MAX_BYTES as usize
    {
        return Err(LegislationError::InvalidProposerRoleCode);
    }

    let actor_cid_number = resolve_institution_cid_number(proposer_code, &resolve_cid_number)?;
    let mut houses = Vec::with_capacity(routing.houses.len());
    for code in &routing.houses {
        houses.push(resolve_institution_cid_number(*code, &resolve_cid_number)?);
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
                input.scope_code,
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
// 法律服务夹具异常应直接中止测试，断言式解包不进入生产路径。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::domains::legislation::law::model::{LawChapter, LawSection};

    /// 夹具解析器:任意机构码 → 确定性机构 CID。
    fn fixture_resolver(code: &[u8; 4]) -> Option<String> {
        Some(format!(
            "LN001-{}0G-000000001-2026",
            super::super::model::institution_code_text(code)
        ))
    }

    fn enact_input(tier: u8, vote_type: u8) -> ProposeLawInput {
        ProposeLawInput {
            law_action: LawActionInput::Enact,
            proposer_role_code: "REPRESENTATIVE".to_string(),
            tier,
            scope_code: 0,
            vote_type,
            title: "道路交通安全法".to_string(),
            title_en: Some("Road Traffic Safety Law".to_string()),
            chapters: vec![LawChapter {
                number: 1,
                title: "总则".to_string(),
                title_en: None,
                sections: vec![LawSection {
                    number: 1,
                    title: "定义".to_string(),
                    title_en: None,
                    articles: vec![],
                }],
            }],
            effective_at: 1000,
            law_id: None,
        }
    }

    #[test]
    fn enact_national_builds_call_with_propose_prefix() {
        let input = enact_input(1, 2); // 国家·重要案
        let call = build_propose_law_call(&input, *b"NRP\0", fixture_resolver).expect("build");
        assert_eq!(&call.call_data[..2], &[25, 0]); // pallet 25 call 0(enact)
        assert_eq!(call.action, 0x1900);
    }

    #[test]
    fn municipal_education_proposer_differs_from_voting_house() {
        // 市教委会(CEDU)发起教育案:proposer ≠ houses[0](市立法会 CLEG),路由解耦成立。
        let input = enact_input(3, 1); // 市·常规教育案
        let call = build_propose_law_call(&input, *b"CEDU", fixture_resolver).expect("build");
        assert_eq!(&call.call_data[..2], &[25, 0]);
    }

    #[test]
    fn amend_without_law_id_is_rejected() {
        let mut input = enact_input(1, 2);
        input.law_action = LawActionInput::Amend;
        input.law_id = None;
        assert!(matches!(
            build_propose_law_call(&input, *b"NRP\0", fixture_resolver),
            Err(LegislationError::MissingLawId)
        ));
    }

    #[test]
    fn unresolved_institution_cid_fails_closed() {
        let input = enact_input(1, 2);
        // 解析器恒空 → 任一机构账户解不出即拒。
        let empty = |_: &[u8; 4]| None;
        assert!(matches!(
            build_propose_law_call(&input, *b"NRP\0", empty),
            Err(LegislationError::InstitutionCidUnresolved)
        ));
    }

    #[test]
    fn provincial_education_has_no_routing() {
        let input = enact_input(2, 1); // 省·教育案(不存在)
        assert!(matches!(
            build_propose_law_call(&input, *b"PRP\0", fixture_resolver),
            Err(LegislationError::UnknownRouting)
        ));
    }

    #[test]
    fn scope_precheck_enforces_tier_and_region() {
        // 国家管理员:可发起国家法律(1)与修宪(0),不可发起省级(2)。
        assert!(precheck_legislation_scope(1, 0, 1, 0).is_ok());
        assert!(precheck_legislation_scope(1, 0, 0, 0).is_ok());
        assert_eq!(
            precheck_legislation_scope(1, 0, 2, 100),
            Err(LegislationError::TierNotAllowedForAdmin)
        );
        // 省管理员只能发起本省:scope 码不符即拒。
        assert!(precheck_legislation_scope(2, 100, 2, 100).is_ok());
        assert_eq!(
            precheck_legislation_scope(2, 100, 2, 200),
            Err(LegislationError::ScopeMismatch)
        );
        // 市管理员不可发起国家法律。
        assert_eq!(
            precheck_legislation_scope(3, 500, 1, 0),
            Err(LegislationError::TierNotAllowedForAdmin)
        );
    }

    #[test]
    fn representative_vote_call_targets_legislation_vote_pallet() {
        let call = build_representative_vote_call(42, "REPRESENTATIVE", true);
        assert_eq!(&call.call_data[..2], &[26, 1]); // pallet 26 call 1(cast_representative_vote)
    }
}

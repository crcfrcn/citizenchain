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
fn institution_cid_number(
    code: [u8; 4],
    cid_number: &str,
) -> Result<Vec<u8>, LegislationError> {
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
    let cid_number = resolve_cid_number(&code)
        .ok_or(LegislationError::InstitutionCidUnresolved)?;
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
        || proposer_role_code.len() > entity_primitives::INSTITUTION_ROLE_CODE_MAX_BYTES as usize
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
// 服务回归使用合规测试 CID，不改正式机构常量或 CID 规则。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use codec::Decode;
    use crate::legislation::law::model::{LawChapter, LawSection};

    fn fixture_cid(code: &[u8; 4]) -> String {
        primitives::cid::generator::generate_cid_number(
            primitives::cid::generator::GenerateCidNumberInput {
                public_key: "0x1234",
                p1: "0",
                province_code: "GD",
                province_name: "广东省",
                city_code: "001",
                city_name: "测试市",
                year: "2026",
                institution: super::super::model::institution_code_text(code).as_str(),
            },
        ).expect("valid fixture CID")
    }

    fn fixture_resolver(code: &[u8; 4]) -> Option<String> {
        Some(fixture_cid(code))
    }

    fn enact_input(tier: u8, vote_type: u8) -> ProposeLawInput {
        ProposeLawInput {
            law_action: LawActionInput::Enact,
            proposer_role_code: "REPRESENTATIVE".to_string(),
            tier,
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
    fn enact_national_builds_call_with_original_actor_and_houses() {
        let input = enact_input(1, 2);
        let actor = fixture_cid(b"NRP\0");
        let call = build_propose_law_call(&input, *b"NRP\0", &actor, fixture_resolver).unwrap();
        assert_eq!(&call.call_data[..2], &[25, 0]);
        assert_eq!(call.action, 0x1900);
        let mut bytes = &call.call_data[2..];
        assert_eq!(u8::decode(&mut bytes).unwrap(), 1);
        assert_eq!(primitives::genesis::LawRegion::decode(&mut bytes).unwrap(),
            primitives::genesis::LawRegion::NATIONAL);
        let houses = Vec::<Vec<u8>>::decode(&mut bytes).unwrap();
        assert_eq!(houses, vec![actor.as_bytes().to_vec(), fixture_cid(b"NSN\0").into_bytes()]);
        assert_eq!(Vec::<u8>::decode(&mut bytes).unwrap(), actor.as_bytes());
    }

    #[test]
    fn bound_actor_is_reused_without_institution_code_lookup() {
        let input = enact_input(1, 2);
        let actor = fixture_cid(b"NRP\0");
        let resolver = |code: &[u8; 4]| {
            assert_ne!(code, b"NRP\0", "发起机构不得按机构码重新查询");
            fixture_resolver(code)
        };
        assert!(build_propose_law_call(&input, *b"NRP\0", &actor, resolver).is_ok());
    }

    #[test]
    fn municipal_education_keeps_actor_separate_from_voting_house() {
        let input = enact_input(3, 1);
        let actor = fixture_cid(b"CEDU");
        let call = build_propose_law_call(&input, *b"CEDU", &actor, fixture_resolver).unwrap();
        let mut bytes = &call.call_data[2..];
        let _ = u8::decode(&mut bytes).unwrap();
        assert_eq!(primitives::genesis::LawRegion::decode(&mut bytes).unwrap(),
            primitives::genesis::LawRegion::from_codes(3, b"GD", b"001").unwrap());
        assert_eq!(Vec::<Vec<u8>>::decode(&mut bytes).unwrap(),
            vec![fixture_cid(b"CLEG").into_bytes()]);
        assert_eq!(Vec::<u8>::decode(&mut bytes).unwrap(), actor.as_bytes());
    }

    #[test]
    fn malformed_or_wrong_actor_institution_is_rejected() {
        let input = enact_input(1, 2);
        for actor in ["invalid-cid".to_string(), fixture_cid(b"NSN\0")] {
            assert!(matches!(
                build_propose_law_call(&input, *b"NRP\0", &actor, fixture_resolver),
                Err(LegislationError::InstitutionCidUnresolved)
            ));
        }
    }

    #[test]
    fn wrong_resolved_route_institution_is_rejected() {
        let input = enact_input(1, 2);
        let actor = fixture_cid(b"NRP\0");
        assert!(matches!(
            build_propose_law_call(&input, *b"NRP\0", &actor, |_| Some(actor.clone())),
            Err(LegislationError::InstitutionCidUnresolved)
        ));
    }

    #[test]
    fn amend_and_repeal_keep_law_id_and_actor_encoding() {
        let actor = fixture_cid(b"NRP\0");
        for (action, call_index) in [(LawActionInput::Amend, 1), (LawActionInput::Repeal, 2)] {
            let mut input = enact_input(1, 2);
            input.law_action = action;
            input.law_id = Some(42);
            let call = build_propose_law_call(&input, *b"NRP\0", &actor, fixture_resolver).unwrap();
            assert_eq!(&call.call_data[..2], &[25, call_index]);
            let mut bytes = &call.call_data[2..];
            assert_eq!(u64::decode(&mut bytes).unwrap(), 42);
            assert_eq!(Vec::<u8>::decode(&mut bytes).unwrap(), actor.as_bytes());
        }
    }

    #[test]
    fn amend_without_law_id_is_rejected() {
        let mut input = enact_input(1, 2);
        input.law_action = LawActionInput::Amend;
        assert!(matches!(
            build_propose_law_call(&input, *b"NRP\0", &fixture_cid(b"NRP\0"), fixture_resolver),
            Err(LegislationError::MissingLawId)
        ));
    }

    #[test]
    fn unresolved_institution_cid_fails_closed() {
        let input = enact_input(1, 2);
        assert!(matches!(
            build_propose_law_call(&input, *b"NRP\0", &fixture_cid(b"NRP\0"), |_| None),
            Err(LegislationError::InstitutionCidUnresolved)
        ));
    }

    #[test]
    fn provincial_education_has_no_routing() {
        let input = enact_input(2, 1);
        assert!(matches!(
            build_propose_law_call(&input, *b"PRP\0", &fixture_cid(b"PRP\0"), fixture_resolver),
            Err(LegislationError::UnknownRouting)
        ));
    }

    #[test]
    fn enact_province_keeps_letters_in_four_byte_slot() {
        let input = enact_input(2, 2);
        let actor = fixture_cid(b"PRP\0");
        let call = build_propose_law_call(&input, *b"PRP\0", &actor, fixture_resolver).unwrap();
        assert_eq!(&call.call_data[3..7], &[b'G', b'D', 0, 0]);
    }

    #[test]
    fn representative_vote_call_targets_legislation_vote_pallet() {
        let call = build_representative_vote_call(42, "REPRESENTATIVE", true);
        assert_eq!(&call.call_data[..2], &[26, 1]);
    }
}

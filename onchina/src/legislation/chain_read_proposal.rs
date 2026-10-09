//! 提案进度只读投影：链上 `Proposal` + `RepresentativeMetas` + `LegislationMetas` + 计票。
//!
//! OnChina 只搬运链上事实，绝不计票或判断通过。代表表决和法律专属元数据必须分别
//! 解码，避免任免、预算等代表表决被误认为法律业务。

use super::law::model::{house_ref, HouseRef};
use crate::core::chain_url;
use codec::Decode;
use serde::Serialize;
use subxt::{dynamic, OnlineClient, PolkadotConfig};

/// votingengine `Proposal<BlockNumber, AccountId>` 解码镜像(BlockNumber=u32 / AccountId=[u8;32])。
#[derive(Debug, Decode)]
#[allow(dead_code)] // SCALE 解码镜像:未读字段只保序,删字段会破坏 Decode 对齐。
pub struct OnChainProposal {
    /// 提案种类(2 = 立法,PROPOSAL_KIND_LEGISLATION)。
    pub kind: u8,
    /// 阶段(10 院内 / 11 公投 / 12 签署 / 13 会签 / 14 护宪)。
    pub stage: u8,
    /// 状态(投票中 / 通过 / 否决)。
    pub status: u8,
    /// 机构码只用于提案分类/路由,不是机构归属真源。
    pub internal_code: Option<[u8; 4]>,
    /// 发起机构唯一身份；个人多签、公民个人或系统提案为空。
    pub actor_cid_number: Option<Vec<u8>>,
    /// 具体资产账户或个人多签执行账户，不得作为机构身份。
    pub execution_account_id: Option<[u8; 32]>,
    /// 受影响机构 CID 列表，不得替代发起机构 CID。
    pub subject_cid_numbers: Vec<Vec<u8>>,
    pub start: u32,
    pub end: u32,
}

/// 代表机构身份只保存 CID。
type RepresentativeBody = Vec<u8>;

/// `RepresentativeRoute<AccountId>` SCALE 镜像。
#[derive(Debug, Decode)]
pub enum OnChainRepresentativeRoute {
    Single(RepresentativeBody),
    Sequential(Vec<RepresentativeBody>),
}

impl OnChainRepresentativeRoute {
    fn bodies(&self) -> Vec<RepresentativeBody> {
        match self {
            Self::Single(body) => vec![body.clone()],
            Self::Sequential(bodies) => bodies.clone(),
        }
    }
}

/// `RepresentativeVoteRule` SCALE 镜像。
#[derive(Debug, Decode, Clone, Copy)]
pub enum OnChainRepresentativeRule {
    Regular,
    Major,
    Special,
}

impl OnChainRepresentativeRule {
    fn index(self) -> u8 {
        self as u8
    }
}

/// `VoteProcedure` SCALE 镜像。
#[derive(Debug, Decode, Clone, Copy)]
pub enum OnChainVoteProcedure {
    RepresentativeOnly,
    Legislation,
}

impl OnChainVoteProcedure {
    fn index(self) -> u8 {
        self as u8
    }
}

/// legislation-vote `RepresentativeMeta<T>` 完整解码镜像。
#[derive(Debug, Decode)]
pub struct OnChainRepresentativeMeta {
    pub route: OnChainRepresentativeRoute,
    pub current_body: u32,
    pub rule: OnChainRepresentativeRule,
    pub procedure: OnChainVoteProcedure,
}

/// legislation-vote `LegislationMeta<T>` 前缀镜像；公投作用域为尾字段且本投影不读取。
#[derive(Debug, Decode)]
#[allow(dead_code)] // SCALE 解码镜像:未读字段只保序,删字段会破坏 Decode 对齐。
pub struct OnChainLegislationMeta {
    pub executive: RepresentativeBody,
    pub legislature: Option<RepresentativeBody>,
    pub needs_guard: bool,
}

/// `VoteCountU32` 解码镜像（代表机构计票）。
#[derive(Debug, Decode, Default)]
pub struct OnChainVoteCount32 {
    pub yes: u32,
    pub no: u32,
}

/// `VoteCountU64` 解码镜像(公投计票)。
#[derive(Debug, Decode, Default)]
pub struct OnChainVoteCount64 {
    pub yes: u64,
    pub no: u64,
}

/// 计票只读投影(院内 u32 计数统一加宽为 u64 展示)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoteTally {
    pub yes: u64,
    pub no: u64,
}

/// 提案进度只读投影(供操作端进度页与大屏消费;不含计票判定,只搬运链上事实)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegProposalState {
    pub proposal_id: u64,
    /// 提案种类(2 = 立法)。
    pub kind: u8,
    /// 阶段(10 院内 / 11 公投 / 12 签署 / 13 会签 / 14 护宪)。
    pub stage: u8,
    /// 状态(投票中 / 通过 / 否决)。
    pub status: u8,
    /// 常规、重要、特别三类数学规则索引。
    pub representative_rule: u8,
    /// 当前代表机构索引。
    pub current_body: u32,
    /// 0=代表表决终局，1=继续法律专属程序。
    pub vote_procedure: u8,
    pub needs_guard: bool,
    /// 代表机构路线（机构 CID）。
    pub representative_bodies: Vec<HouseRef>,
    /// 阶段起止块。
    pub start_block: u32,
    pub end_block: u32,
    /// 当前代表机构计票。
    pub representative_tally: VoteTally,
    /// 公投计票(非特别案为 0/0)。
    pub referendum_tally: VoteTally,
}

/// 链上 Proposal + 分离元数据 + tally → 只读投影 `LegProposalState`。
pub fn build_leg_proposal_state(
    proposal_id: u64,
    proposal: &OnChainProposal,
    representative_meta: &OnChainRepresentativeMeta,
    legislation_meta: &OnChainLegislationMeta,
    representative_tally: &OnChainVoteCount32,
    referendum_tally: &OnChainVoteCount64,
) -> LegProposalState {
    LegProposalState {
        proposal_id,
        kind: proposal.kind,
        stage: proposal.stage,
        status: proposal.status,
        representative_rule: representative_meta.rule.index(),
        current_body: representative_meta.current_body,
        vote_procedure: representative_meta.procedure.index(),
        needs_guard: legislation_meta.needs_guard,
        representative_bodies: representative_meta
            .route
            .bodies()
            .iter()
            .map(|cid_number| house_ref(cid_number))
            .collect(),
        start_block: proposal.start,
        end_block: proposal.end,
        representative_tally: VoteTally {
            yes: representative_tally.yes as u64,
            no: representative_tally.no as u64,
        },
        referendum_tally: VoteTally {
            yes: referendum_tally.yes,
            no: referendum_tally.no,
        },
    }
}

/// 按明确存储键点查并解码，双 Map 的代表计票同时传入提案 ID 和机构索引。
async fn fetch_value<V: Decode>(
    pallet: &str,
    item: &str,
    keys: Vec<dynamic::Value>,
) -> Result<Option<V>, String> {
    let ws_url = chain_url::chain_ws_url()?;
    let client = OnlineClient::<PolkadotConfig>::from_insecure_url(ws_url.as_str())
        .await
        .map_err(|e| format!("connect chain ws for {pallet}::{item} failed: {e}"))?;
    let storage = client
        .storage()
        .at_latest()
        .await
        .map_err(|e| format!("get latest chain storage failed: {e}"))?;
    let query = dynamic::storage(pallet, item, keys);
    let Some(value) = storage
        .fetch(&query)
        .await
        .map_err(|e| format!("fetch {pallet}::{item} failed: {e}"))?
    else {
        return Ok(None);
    };
    let mut raw = value.encoded();
    V::decode(&mut raw)
        .map(Some)
        .map_err(|e| format!("decode {pallet}::{item} failed: {e}"))
}

/// 读取法律提案完整投影（两类元数据必存，tally 缺省 0/0）。
pub async fn fetch_proposal_state(proposal_id: u64) -> Result<Option<LegProposalState>, String> {
    let proposal_key = || vec![dynamic::Value::u128(proposal_id as u128)];
    let Some(proposal) =
        fetch_value::<OnChainProposal>("VotingEngine", "Proposals", proposal_key()).await?
    else {
        return Ok(None);
    };
    let Some(representative_meta) = fetch_value::<OnChainRepresentativeMeta>(
        "LegislationVote",
        "RepresentativeMetas",
        proposal_key(),
    )
    .await?
    else {
        return Ok(None);
    };
    // 本接口只投影法律提案；纯代表表决没有 LegislationMetas，直接跳过。
    let Some(legislation_meta) = fetch_value::<OnChainLegislationMeta>(
        "LegislationVote",
        "LegislationMetas",
        proposal_key(),
    )
    .await?
    else {
        return Ok(None);
    };
    let representative_tally = fetch_value::<OnChainVoteCount32>(
        "LegislationVote",
        "RepresentativeTallies",
        vec![
            dynamic::Value::u128(proposal_id as u128),
            dynamic::Value::u128(representative_meta.current_body as u128),
        ],
    )
    .await?
    .unwrap_or_default();
    let referendum_tally =
        fetch_value::<OnChainVoteCount64>("LegislationVote", "LegReferendumTally", proposal_key())
            .await?
            .unwrap_or_default();
    Ok(Some(build_leg_proposal_state(
        proposal_id,
        &proposal,
        &representative_meta,
        &legislation_meta,
        &representative_tally,
        &referendum_tally,
    )))
}

#[cfg(test)]
// 提案 SCALE 夹具必须精确解码，断言式解包用于暴露链契约变化。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use codec::Encode;

    /// Proposal 镜像从链端字段序编码的 golden 逐字节解码一致。
    #[test]
    fn proposal_decodes_from_field_ordered_golden() {
        let mut golden = Vec::new();
        golden.extend(2u8.encode()); // kind = 立法
        golden.extend(10u8.encode()); // stage = 院内
        golden.extend(0u8.encode()); // status = 投票中
        golden.extend(Some(*b"NRP\0").encode()); // internal_code
        golden.extend(Some(b"LN001-NRP0G-000000001-2026".to_vec()).encode()); // actor_cid_number
        golden.extend(Option::<[u8; 32]>::None.encode()); // execution_account_id
        golden.extend(vec![b"LN001-NRP0G-000000001-2026".to_vec()].encode()); // subject_cid_numbers
        golden.extend(100u32.encode()); // start
        golden.extend(200u32.encode()); // end

        let proposal = OnChainProposal::decode(&mut &golden[..]).expect("decode Proposal");
        assert_eq!(proposal.kind, 2);
        assert_eq!(proposal.stage, 10);
        assert_eq!(proposal.internal_code, Some(*b"NRP\0"));
        assert_eq!(
            proposal.actor_cid_number.as_deref(),
            Some(b"LN001-NRP0G-000000001-2026".as_slice())
        );
        assert_eq!(proposal.subject_cid_numbers.len(), 1);
        assert_eq!(proposal.start, 100);
    }

    /// 代表元数据与法律元数据按两个独立存储布局解码。
    #[test]
    fn split_meta_mirrors_decode_independently() {
        let bodies: Vec<RepresentativeBody> = vec![
            b"LN001-NRP0G-000000001-2026".to_vec(),
            b"LN001-NSN0G-000000001-2026".to_vec(),
        ];
        let mut representative_golden = Vec::new();
        representative_golden.extend(1u8.encode()); // Sequential
        representative_golden.extend(bodies.encode());
        representative_golden.extend(0u32.encode()); // current_body
        representative_golden.extend(1u8.encode()); // Major
        representative_golden.extend(1u8.encode()); // Legislation
        let representative = OnChainRepresentativeMeta::decode(&mut &representative_golden[..])
            .expect("decode RepresentativeMeta");
        assert_eq!(representative.current_body, 0);
        assert_eq!(representative.rule.index(), 1);
        assert_eq!(representative.route.bodies().len(), 2);

        let mut legislation_golden = Vec::new();
        legislation_golden.extend(b"LN001-PRS0G-000000001-2026".to_vec().encode());
        legislation_golden.extend(Some(b"LN001-NLG0G-000000001-2026".to_vec()).encode());
        legislation_golden.extend(false.encode());
        legislation_golden.extend(Option::<()>::None.encode()); // referendum_scope 尾字段
        let legislation = OnChainLegislationMeta::decode(&mut &legislation_golden[..])
            .expect("decode LegislationMeta prefix");
        assert_eq!(legislation.executive, b"LN001-PRS0G-000000001-2026");
        assert_eq!(
            legislation.legislature.as_deref(),
            Some(b"LN001-NLG0G-000000001-2026".as_slice())
        );
        assert!(!legislation.needs_guard);
    }

    /// VoteCount 镜像解码 + 组装 LegProposalState(计票加宽、houses→HouseRef)。
    #[test]
    fn build_state_projects_tally_and_houses() {
        let proposal = OnChainProposal {
            kind: 2,
            stage: 10,
            status: 0,
            internal_code: None,
            actor_cid_number: None,
            execution_account_id: None,
            subject_cid_numbers: Vec::new(),
            start: 100,
            end: 200,
        };
        let representative_meta = OnChainRepresentativeMeta {
            route: OnChainRepresentativeRoute::Sequential(vec![
                b"LN001-NRP0G-000000001-2026".to_vec(),
                b"LN001-NSN0G-000000001-2026".to_vec(),
            ]),
            current_body: 1,
            rule: OnChainRepresentativeRule::Major,
            procedure: OnChainVoteProcedure::Legislation,
        };
        let legislation_meta = OnChainLegislationMeta {
            executive: b"LN001-PRS0G-000000001-2026".to_vec(),
            legislature: Some(b"LN001-NLG0G-000000001-2026".to_vec()),
            needs_guard: false,
        };
        let representative_tally = OnChainVoteCount32 { yes: 220, no: 30 };
        let referendum_tally = OnChainVoteCount64::default();

        let state = build_leg_proposal_state(
            7,
            &proposal,
            &representative_meta,
            &legislation_meta,
            &representative_tally,
            &referendum_tally,
        );
        assert_eq!(state.proposal_id, 7);
        assert_eq!(state.stage, 10);
        assert_eq!(state.current_body, 1);
        assert_eq!(state.representative_bodies.len(), 2);
        assert_eq!(
            state.representative_bodies[1].cid_number,
            "LN001-NSN0G-000000001-2026"
        );
        assert_eq!(state.representative_tally.yes, 220);
        assert_eq!(state.representative_tally.no, 30);
        assert_eq!(state.referendum_tally.yes, 0);
    }
}

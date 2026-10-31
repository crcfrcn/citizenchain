//! 名册 × 活跃提案 × 逐席投票的聚合装配。
//!
//! 大屏看板的纯装配层——把链读结果拼成 `DisplayBoard`。计票/阶段判定不在此发生
//! (只搬运 `fetch_proposal_state` 的只读投影)。席位投票由本机构名册左连接逐席投票映射得到。

use std::collections::HashMap;

use crate::chain_read_proposal::{fetch_proposal_state, LegProposalState};
use onchina::chain_runtime::NodeInstitutionIdentity;
use onchina::institutions::admins::chain_roles::{
    fetch_active_assignments_onchain, InstitutionAssignmentView,
};

use super::chain_read::{fetch_active_proposal_ids, fetch_representative_ballots, BallotKey};
use super::model::{ActiveProposalView, DisplayBoard, SeatView};

/// 立法提案种类判别式(对齐链端 votebase `PROPOSAL_KIND_LEGISLATION`)。
const PROPOSAL_KIND_LEGISLATION: u8 = 2;

/// 装配本节点机构的大屏看板:名册 + 活跃立法提案(逐席投票)。
///
/// 活跃提案来自 `ActiveProposalsBySubject[InstitutionCid(cid_number)]`;逐个取进度投影,
/// 非法律提案（无 `LegislationMetas`）或已清理者跳过。
pub(crate) async fn build_display_board(
    identity: &NodeInstitutionIdentity,
    institution_code: String,
    cid_short_name: Option<String>,
    scope_label: String,
) -> Result<DisplayBoard, String> {
    let roster = fetch_active_assignments_onchain(identity)
        .await?
        .unwrap_or_default();
    // FRG 是唯一机构身份；其任职地域不产生立法提案。
    let active_ids = if onchina::chain_runtime::is_tier1_registry(&institution_code) {
        Vec::new()
    } else {
        fetch_active_proposal_ids(&identity.cid_number).await?
    };

    let mut active_proposals = Vec::new();
    for proposal_id in active_ids {
        let Some(state) = fetch_proposal_state(proposal_id).await? else {
            continue;
        };
        if state.kind != PROPOSAL_KIND_LEGISLATION {
            continue;
        }
        let ballots = fetch_representative_ballots(proposal_id, state.current_body).await?;
        active_proposals.push(build_active_proposal_view(
            state,
            &identity.cid_number,
            &roster,
            &ballots,
        ));
    }

    Ok(DisplayBoard {
        institution_code,
        cid_short_name,
        scope_label,
        roster_total: roster.len() as u32,
        active_proposals,
    })
}

/// 名册左连接逐席投票 → 席位板 + 聚合计数(纯装配,可单测)。
fn build_active_proposal_view(
    state: LegProposalState,
    institution_cid: &str,
    roster: &[InstitutionAssignmentView],
    ballots: &HashMap<BallotKey, bool>,
) -> ActiveProposalView {
    let mut seen = std::collections::HashSet::new();
    let seats: Vec<SeatView> = roster
        .iter()
        .filter_map(|p| {
            let key = BallotKey {
                proposal_id: state.proposal_id,
                body_index: state.current_body,
                institution_cid_number: institution_cid.to_string(),
                role_code: p.role_code.clone(),
                account_id: p.account_id.clone(),
            };
            if !seen.insert(key.clone()) {
                return None;
            }
            Some(SeatView {
                ticket_id: key.seat_id(),
                role_code: p.role_code.clone(),
                account_id: p.account_id.clone(),
                name: String::new(),
                role_name: p.role_name.clone(),
                vote: ballots.get(&key).copied(),
            })
        })
        .collect();
    let approved_count = seats.iter().filter(|s| s.vote == Some(true)).count() as u32;
    let rejected_count = seats.iter().filter(|s| s.vote == Some(false)).count() as u32;
    let pending_count = seats.iter().filter(|s| s.vote.is_none()).count() as u32;
    ActiveProposalView {
        state,
        seats,
        approved_count,
        rejected_count,
        pending_count,
    }
}

#[cfg(test)]
#[path = "../tests/display_service.rs"]
mod tests;

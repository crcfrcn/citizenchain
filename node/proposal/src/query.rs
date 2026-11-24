//! 读取提案编号、分页索引及机构管理提案的链状态。

use nodeupgrade::types::RuntimeUpgradeDetail;

use noderesolutionissue::types::ResolutionIssuanceDetail;

use nodedestroy::types::ResolutionDestroyDetail;

use nodeclient::codec::decode_hex_storage;

use nodeupgrade::decode::decode_runtime_upgrade_action;

use noderesolutionissue::decode::decode_resolution_issuance_action;

use nodedestroy::decode::decode_resolution_destroy_action;

use nodeclient::codec::decode_u128_vec;

use nodeclient::codec::read_compact_u32;

use nodeclient::codec::decode_cid_number;

use nodeclient::codec::decode_subject_cid_numbers;

use primitives::cid::code::InstitutionCode;

use codec::{DecodeAll, Encode};

use crate::display::fetch_proposal_display;
use crate::display::format_proposal_id;
use crate::display::kind_label;
use crate::display::resolve_cid_full_name_by_subjects;
use crate::display::stage_label;
use crate::display::status_label;
use crate::display::ProposalDisplayInfo;
use crate::types::FeeRateProposalDetail;
use crate::types::ProposalAction;
use crate::types::ProposalDisplayMeta;
use crate::types::ProposalFullInfo;
use crate::types::ProposalListItem;
use crate::types::ProposalMeta;
use crate::types::ProposalPageResult;
use crate::types::VoterRoleSubject;
use entitybase::AuthorizationSubject;

/// 机构管理提案 TAG — 不属于治理提案，在治理列表中过滤掉。
/// 机构管理已拆分公权/私权两 pallet,各自 MODULE_TAG:publicentity=`pub-mgmt`、privateentity=`pri-mgmt`。
const TAG_PUBLIC_MANAGE: &[u8] = b"pub-mgmt";

const TAG_PRIVATE_MANAGE: &[u8] = b"pri-mgmt";

// ──── 公开查询函数 ────

/// 查询 NextProposalId（VotingEngine 全局递增 ID）。
pub(crate) fn fetch_next_proposal_id() -> Result<u128, String> {
    let key = nodeclient::query::value_key("VotingEngine", "NextProposalId");
    // (ADR-017):业务读取统一经 chain_query 钉 finalized,禁止 best。
    match nodeclient::query::fetch_finalized_storage(&key)? {
        None => Ok(0),
        Some(hex_data) => {
            let data = decode_hex_storage(&hex_data)?;
            decode_next_proposal_id(&data)
        }
    }
}

/// 已存在的计数器必须恰好为 u128；坏数据不能冒充未分配过的零值。
fn decode_next_proposal_id(data: &[u8]) -> Result<u128, String> {
    let encoded = <[u8; 16]>::try_from(data)
        .map_err(|_| "NextProposalId 长度无效，必须为16字节".to_string())?;
    Ok(u128::from_le_bytes(encoded))
}

/// 倒序分页包含零号，checked_sub 同时处理接近零和 u128 上限的边界。
fn proposal_page_ids(start_id: u128, count: u32) -> (impl Iterator<Item = u128>, bool) {
    let has_more = count > 0 && start_id.checked_sub(u128::from(count)).is_some();
    (
        (0..count).map_while(move |offset| start_id.checked_sub(u128::from(offset))),
        has_more,
    )
}

/// 检查提案是否为机构管理提案（创建/关闭机构多签账户），这类提案不在治理列表中显示。
/// 机构管理已拆公权/私权两 pallet,命中 `pub-mgmt` 或 `pri-mgmt` 任一前缀即认定。
fn is_institution_manage_proposal(proposal_id: u128) -> bool {
    let Ok(Some(raw)) = fetch_proposal_data_raw(proposal_id) else {
        return false;
    };
    if raw.is_empty() {
        return false;
    }
    // ProposalData 存储为 BoundedVec<u8>：Compact<len> + bytes
    let Ok((vec_len, len_bytes)) = read_compact_u32(&raw, 0) else {
        return false;
    };
    let offset = len_bytes;
    [TAG_PUBLIC_MANAGE, TAG_PRIVATE_MANAGE].iter().any(|tag| {
        offset + tag.len() <= raw.len()
            && (vec_len as usize) >= tag.len()
            && raw[offset..offset + tag.len()] == **tag
    })
}

/// 分页查询提案列表(从 start_id 往前 count 个,按 ID 倒序)。
/// 自动过滤多签管理提案(创建/关闭多签账户),这些在多签账户详情页单独展示。
///
/// 双层 ID：主键从 0 起单调累加，所以下界用 0（不再按年份切）。
pub(crate) fn fetch_proposal_page(
    start_id: u128,
    count: u32,
) -> Result<ProposalPageResult, String> {
    let mut items = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let (ids, has_more) = proposal_page_ids(start_id, count);
    for id in ids {
        match fetch_proposal_meta(id) {
            Ok(Some(meta)) => {
                // 多签管理提案(创建/关闭多签账户)不在治理列表中显示。
                if is_institution_manage_proposal(id) {
                    continue;
                }
                // 协议升级摘要只保存展示字段，真实状态统一读取 engingcore。
                let display = match fetch_proposal_display(id, &meta) {
                    Ok(v) => v,
                    Err(_) => ProposalDisplayInfo {
                        summary: "(详情查询失败)".to_string(),
                        card: None,
                        status: meta.status,
                        status_label: status_label(meta.status).to_string(),
                    },
                };
                let cid_full_name = resolve_cid_full_name_by_subjects(&meta.subject_cid_numbers);
                // 双层 ID：展示号从 ProposalDisplayId 反查；查不到时回退为 `#id`。
                let display_meta = fetch_proposal_display_id(id).ok().flatten();

                items.push(ProposalListItem {
                    proposal_id: id,
                    display_id: format_proposal_id(id, display_meta.as_ref()),
                    kind: meta.kind,
                    kind_label: kind_label(meta.kind).to_string(),
                    stage: meta.stage,
                    stage_label: stage_label(meta.stage).to_string(),
                    status: display.status,
                    status_label: display.status_label,
                    cid_full_name,
                    summary: display.summary,
                    card: display.card,
                });
            }
            Ok(None) => {} // 提案不存在,跳过
            Err(e) => {
                warnings.push(format!("查询提案 {id} 失败: {e}"));
            }
        }
    }

    Ok(ProposalPageResult {
        items,
        has_more,
        warning: if warnings.is_empty() {
            None
        } else {
            Some(warnings.join(";"))
        },
    })
}

/// 查询单个提案完整信息。
///
/// 业务动作统一走 [`resolve_proposal_action`] 解析,再按变体填入对应 `*_detail` 字段;
/// 投票计数按 kind/stage 独立查询(保留原有条件)。
pub(crate) fn fetch_proposal_full(proposal_id: u128) -> Result<ProposalFullInfo, String> {
    let at = nodeclient::query::fetch_finalized_head()?;
    let meta = fetch_proposal_meta_at(proposal_id, &at)?
        .ok_or_else(|| format!("提案 {proposal_id} 不存在"))?;

    // 业务动作:一次性解析,按变体分发到 7 个 *_detail 字段。
    let action = resolve_proposal_action(proposal_id, &meta)?;
    let (
        business_details,
        runtime_upgrade_detail,
        resolution_issuance_detail,
        resolution_destroy_detail,
        fee_rate_detail,
    ) = split_action_into_details(action);

    let internal_tally = if meta.kind == 0 || meta.stage == 0 {
        Some(crate::tally::fetch_internal_tally_at(proposal_id, &at)?)
    } else {
        None
    };
    let joint_tally = if meta.kind == 1 && meta.stage >= 1 {
        Some(crate::tally::fetch_joint_tally_at(proposal_id, &at)?)
    } else {
        None
    };
    let referendum_tally = if meta.kind == 1 && meta.stage >= 2 {
        Some(crate::tally::fetch_referendum_tally_at(proposal_id, &at)?)
    } else {
        None
    };
    let cid_full_name = resolve_cid_full_name_by_subjects(&meta.subject_cid_numbers);
    let plan = read_plan_at(proposal_id, &at)?;
    let voter_role_subjects = plan
        .voter_subjects
        .iter()
        .filter_map(|s| match s {
            AuthorizationSubject::Institution(r) => Some(VoterRoleSubject {
                cid_number: String::from_utf8_lossy(&r.cid_number).into_owned(),
                role_code: String::from_utf8_lossy(&r.role_code).into_owned(),
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    let frozen_vote_view = if voter_role_subjects.is_empty() {
        None
    } else {
        Some(load_frozen_vote_view_at(proposal_id, &at)?)
    };

    Ok(ProposalFullInfo {
        meta,
        business_details,
        runtime_upgrade_detail,
        fee_rate_detail,
        resolution_issuance_detail,
        resolution_destroy_detail,
        internal_tally,
        joint_tally,
        referendum_tally,
        voter_role_subjects,
        frozen_vote_view,
        cid_full_name,
    })
}

/// 把 [`ProposalAction`] 展开成 `ProposalFullInfo` 的业务详情字段。
///
/// 业务模块详情由共享适配层承载，再 flatten 到接口返回值中；
/// 其它治理详情仍保留在本聚合层。
#[allow(clippy::type_complexity)]
fn split_action_into_details(
    action: ProposalAction,
) -> (
    nodemultisig::types::ProposalDetails,
    Option<RuntimeUpgradeDetail>,
    Option<ResolutionIssuanceDetail>,
    Option<ResolutionDestroyDetail>,
    Option<FeeRateProposalDetail>,
) {
    match action {
        crate::types::ProposalAction::Business(d) => ((*d).into_details(), None, None, None, None),
        crate::types::ProposalAction::RuntimeUpgrade(d) => (
            nodemultisig::types::ProposalDetails::default(),
            Some(*d),
            None,
            None,
            None,
        ),
        crate::types::ProposalAction::ResolutionIssuance(d) => (
            nodemultisig::types::ProposalDetails::default(),
            None,
            Some(*d),
            None,
            None,
        ),
        crate::types::ProposalAction::ResolutionDestroy(d) => (
            nodemultisig::types::ProposalDetails::default(),
            None,
            None,
            Some(*d),
            None,
        ),
        crate::types::ProposalAction::FeeRate(d) => (
            nodemultisig::types::ProposalDetails::default(),
            None,
            None,
            None,
            Some(*d),
        ),
        crate::types::ProposalAction::Unknown => (
            nodemultisig::types::ProposalDetails::default(),
            None,
            None,
            None,
            None,
        ),
    }
}

/// 分页查询指定机构的所有存在提案（从 start_id 往前，按 ID 倒序）。
///
/// 通过机构 CID 反向索引读取本机构提案。
/// 每页最多返回 count 条，has_more 表示是否还有更早的提案。
pub(crate) fn fetch_institution_proposal_page(
    cid_number: &str,
    start_id: u128,
    count: u32,
) -> Result<ProposalPageResult, String> {
    let mut items = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // 双层 ID：走 ProposalsByCid 反向索引，复杂度为 O（本机构提案数），
    // 不再扫主键 + 客户端过滤。
    let mut ids = fetch_proposals_by_cid(cid_number)?;
    ids.sort_by(|a, b| b.cmp(a)); // 降序(主键单调,降序即按时间倒序)

    // start_id 是上一次翻页返回的最后一个 id - 1。本次取 ids 中 ≤ start_id 的部分。
    let from_idx = ids
        .iter()
        .position(|id| *id <= start_id)
        .unwrap_or(ids.len());
    let take_ids: Vec<u128> = ids
        .iter()
        .skip(from_idx)
        .take(count as usize)
        .copied()
        .collect();
    let next_idx = from_idx + take_ids.len();

    for id in take_ids {
        if is_institution_manage_proposal(id) {
            // 防御性过滤：多签管理提案不属于治理机构列表，详情页不展示。
            continue;
        }
        match fetch_proposal_meta(id) {
            Ok(Some(meta)) => {
                let display = match fetch_proposal_display(id, &meta) {
                    Ok(v) => v,
                    Err(_) => ProposalDisplayInfo {
                        summary: "(详情查询失败)".to_string(),
                        card: None,
                        status: meta.status,
                        status_label: status_label(meta.status).to_string(),
                    },
                };
                let cid_full_name = resolve_cid_full_name_by_subjects(&meta.subject_cid_numbers);
                let display_meta = fetch_proposal_display_id(id).ok().flatten();
                items.push(ProposalListItem {
                    proposal_id: id,
                    display_id: format_proposal_id(id, display_meta.as_ref()),
                    kind: meta.kind,
                    kind_label: kind_label(meta.kind).to_string(),
                    stage: meta.stage,
                    stage_label: stage_label(meta.stage).to_string(),
                    status: display.status,
                    status_label: display.status_label,
                    cid_full_name,
                    summary: display.summary,
                    card: display.card,
                });
            }
            Ok(None) => {} // 提案不存在,跳过
            Err(e) => {
                warnings.push(format!("查询提案 {id} 失败: {e}"));
            }
        }
    }

    let has_more = next_idx < ids.len();

    Ok(ProposalPageResult {
        items,
        has_more,
        warning: if warnings.is_empty() {
            None
        } else {
            Some(warnings.join("；"))
        },
    })
}

/// 查询机构的活跃提案 ID 列表。
pub(crate) fn fetch_active_proposal_ids(cid_number: &str) -> Result<Vec<u128>, String> {
    let subject_key = proposal_subject_institution_cid_key(cid_number)?;
    let key = nodeclient::query::map_key("VotingEngine", "ActiveProposalsBySubject", &subject_key);
    // (ADR-017):活跃提案索引按 finalized 口径读取,避免 best 漂移漏列。
    match nodeclient::query::fetch_finalized_storage(&key)? {
        None => Ok(Vec::new()),
        Some(hex_data) => {
            let data = decode_hex_storage(&hex_data)?;
            decode_u128_vec(&data)
        }
    }
}

// ──── 内部查询 ────

pub(crate) fn fetch_proposal_meta(proposal_id: u128) -> Result<Option<ProposalMeta>, String> {
    let at = nodeclient::query::fetch_finalized_head()?;
    fetch_proposal_meta_at(proposal_id, &at)
}
pub(crate) fn fetch_proposal_meta_at(
    proposal_id: u128,
    at: &str,
) -> Result<Option<ProposalMeta>, String> {
    let key = nodeclient::query::map_key("VotingEngine", "Proposals", &proposal_id.to_le_bytes());
    nodeclient::query::fetch_storage_at(&key, at)?
        .map(|raw| {
            let data = decode_hex_storage(&raw)?;
            decode_proposal_meta(proposal_id, &data).ok_or_else(|| "提案元数据损坏".into())
        })
        .transpose()
}

fn fetch_proposal_data_raw(proposal_id: u128) -> Result<Option<Vec<u8>>, String> {
    let key =
        nodeclient::query::map_key("VotingEngine", "ProposalData", &proposal_id.to_le_bytes());
    // 提案动作里包含金额字段,详情和摘要按 finalized 口径展示。
    match nodeclient::query::fetch_finalized_storage(&key)? {
        None => Ok(None),
        Some(hex_data) => {
            let data = decode_hex_storage(&hex_data)?;
            Ok(Some(data))
        }
    }
}

/// 按优先级依次查询所有提案动作来源,返回命中的第一个业务动作。
///
/// 查找顺序(命中即返回,不重复查询):
/// 1. `VotingEngine::ProposalData`(业务模块/升级/发行/销毁,按 kind 分流)
/// 2. 业务模块独立 storage 动作
/// 3. 全部未命中 → [`ProposalAction::Unknown`]
///
/// 常见提案(业务模块/升级/销毁/发行)1 次 RPC 即可命中;少量业务 detail
/// 存在独立 pallet 存储,会多查 1~2 次,但这几类提案频率极低。
pub(crate) fn resolve_proposal_action(
    proposal_id: u128,
    meta: &ProposalMeta,
) -> Result<ProposalAction, String> {
    // ── Step 1:尝试从 VotingEngine::ProposalData 解码 ──
    if let Some(raw) = fetch_proposal_data_raw(proposal_id)? {
        if !raw.is_empty() {
            // ProposalData 存储为 BoundedVec<u8>:Compact<len> + bytes
            let (vec_len, len_bytes) = read_compact_u32(&raw, 0)?;
            let offset = len_bytes;
            if offset + vec_len as usize <= raw.len() {
                let data = &raw[offset..offset + vec_len as usize];
                if meta.kind == 1 {
                    // 联合投票:先 runtime 升级,再决议发行
                    if let Some(detail) = decode_runtime_upgrade_action(proposal_id, data) {
                        return Ok(crate::types::ProposalAction::RuntimeUpgrade(Box::new(
                            detail,
                        )));
                    }
                    if let Some(detail) = decode_resolution_issuance_action(proposal_id, data) {
                        return Ok(crate::types::ProposalAction::ResolutionIssuance(Box::new(
                            detail,
                        )));
                    }
                } else {
                    // 内部投票:先交给业务模块适配层识别,再识别治理侧销毁。
                    if let Some(action) =
                        crate::business::decode_internal_proposal_data_action(proposal_id, data)
                    {
                        return Ok(crate::types::ProposalAction::Business(Box::new(action)));
                    }
                    if let Some(detail) = decode_resolution_destroy_action(proposal_id, data) {
                        return Ok(crate::types::ProposalAction::ResolutionDestroy(Box::new(
                            detail,
                        )));
                    }
                }
            }
        }
    }

    // ── Step 2:业务模块独立 storage ──
    if let Some(action) = crate::business::fetch_stored_action(proposal_id)? {
        return Ok(crate::types::ProposalAction::Business(Box::new(action)));
    }

    // Step 3:全部未命中
    Ok(crate::types::ProposalAction::Unknown)
}

fn decode_proposal_meta(proposal_id: u128, data: &[u8]) -> Option<ProposalMeta> {
    if data.len() < 3 {
        return None;
    }
    let kind = data[0];
    let stage = data[1];
    let status = data[2];

    let mut offset = 3;

    // internal_code: Option<InstitutionCode>([u8;4])
    let internal_code = if offset < data.len() && data[offset] == 1 {
        offset += 1;
        if offset + 4 <= data.len() {
            let code: InstitutionCode = data[offset..offset + 4].try_into().ok()?;
            offset += 4;
            Some(code)
        } else {
            None
        }
    } else {
        offset += 1; // skip 0x00 (None)
        None
    };

    // actor_cid_number: Option<CidNumber>
    let actor_cid_number = if offset < data.len() && data[offset] == 1 {
        offset += 1;
        Some(decode_cid_number(data, &mut offset)?)
    } else {
        offset += 1; // skip 0x00 (None)
        None
    };
    // execution_account_id: Option<AccountId32>
    let execution_account_id = if offset < data.len() && data[offset] == 1 {
        offset += 1;
        let end = offset.checked_add(32)?;
        if end > data.len() {
            return None;
        }
        let value = Some(format!("0x{}", hex::encode(&data[offset..end])));
        offset = end;
        value
    } else {
        offset += 1;
        None
    };
    let (subject_cid_numbers, _offset) = decode_subject_cid_numbers(data, offset)?;

    Some(ProposalMeta {
        proposal_id,
        kind,
        stage,
        status,
        internal_code,
        actor_cid_number,
        execution_account_id,
        subject_cid_numbers,
    })
}

pub(crate) fn encode_compact_u32(value: u32) -> Vec<u8> {
    if value < (1 << 6) {
        vec![(value as u8) << 2]
    } else if value < (1 << 14) {
        (((value << 2) | 0b01) as u16).to_le_bytes().to_vec()
    } else {
        ((value << 2) | 0b10).to_le_bytes().to_vec()
    }
}

fn scale_bounded_bytes(raw: &[u8]) -> Vec<u8> {
    let mut out = encode_compact_u32(raw.len() as u32);
    out.extend_from_slice(raw);
    out
}

pub(crate) fn cid_number_key(cid_number: &str) -> Result<Vec<u8>, String> {
    let trimmed = cid_number.trim();
    if trimmed.is_empty() {
        return Err("机构 CID 不能为空".to_string());
    }
    Ok(scale_bounded_bytes(trimmed.as_bytes()))
}

fn proposal_subject_institution_cid_key(cid_number: &str) -> Result<Vec<u8>, String> {
    let mut out = vec![0u8]; // ProposalSubject::InstitutionCid(CidNumber)
    out.extend(cid_number_key(cid_number)?);
    Ok(out)
}

/// 查询 `VotingEngine::ProposalDisplayId[id]` → `ProposalDisplayMeta { year:u32, seq_in_year:u64 }`。
/// 12 字节 SCALE:u32 LE + u64 LE。
pub(crate) fn fetch_proposal_display_id(
    proposal_id: u128,
) -> Result<Option<ProposalDisplayMeta>, String> {
    let key = nodeclient::query::map_key(
        "VotingEngine",
        "ProposalDisplayId",
        &proposal_id.to_le_bytes(),
    );
    // (ADR-017):展示号反查按 finalized 口径读取,禁止 best。
    let hex_value = match nodeclient::query::fetch_finalized_storage(&key)? {
        Some(s) => s,
        None => return Ok(None),
    };
    let raw = hex::decode(hex_value.trim_start_matches("0x"))
        .map_err(|e| format!("解析 ProposalDisplayId hex 失败: {e}"))?;
    if raw.len() != 12 {
        return Ok(None);
    }
    let year = u32::from_le_bytes(raw[..4].try_into().map_err(|_| "展示年份长度错误")?);
    let seq_in_year = u64::from_le_bytes(raw[4..12].try_into().map_err(|_| "年内序号长度错误")?);
    Ok(Some(ProposalDisplayMeta { year, seq_in_year }))
}

/// 通用反向索引迭代器:列举 `StorageDoubleMap<_, Twox64Concat, K1, Twox64Concat, u128, ()>`
/// 在指定 K1 下的所有 proposal_id。每条 key 末 16 字节 = u128 LE = proposal_id。
fn fetch_proposal_ids_by_index(storage_name: &str, key1: &[u8]) -> Result<Vec<u128>, String> {
    let prefix = nodeclient::query::twox64_concat_prefix("VotingEngine", storage_name, key1);
    // (ADR-017):反向索引列举按 finalized 口径,best 漂移会列出半新半旧 key 集。
    let keys = nodeclient::query::fetch_finalized_keys_paged(&prefix, 1000, None)?;
    let mut ids = Vec::with_capacity(keys.len());
    for s in &keys {
        let bytes = match hex::decode(s.trim_start_matches("0x")) {
            Ok(b) => b,
            Err(_) => continue,
        };
        if bytes.len() < 16 {
            continue;
        }
        let mut tail = [0u8; 16];
        tail.copy_from_slice(&bytes[bytes.len() - 16..]);
        ids.push(u128::from_le_bytes(tail));
    }
    Ok(ids)
}

/// 反向索引:`ProposalsByCode[institution_code]` → 该机构码下所有 proposal_id。
///
/// 链上第一腿 key 已从 `org: u8`(1 字节)改为 `InstitutionCode`([u8;4]),
/// `institution_code` 入参是 4 字符机构码字符串(如 "NRC"/"CGOV")。
pub(crate) fn fetch_proposals_by_institution_code(
    institution_code: &str,
) -> Result<Vec<u128>, String> {
    let code = primitives::cid::code::code_bytes(institution_code.trim());
    fetch_proposal_ids_by_index("ProposalsByCode", &code)
}

/// 反向索引:`ProposalsByCid[cid_number]` → 本机构主体所有关联 proposal_id。
pub(crate) fn fetch_proposals_by_cid(cid_number: &str) -> Result<Vec<u128>, String> {
    let cid_key = cid_number_key(cid_number)?;
    fetch_proposal_ids_by_index("ProposalsByCid", &cid_key)
}

/// 反向索引:`ProposalsByOwner[module_tag]` → 该业务模块所有 proposal_id。
/// `module_tag` 入参为 BoundedVec<u8> 的 SCALE 编码体(Compact<len> + bytes)。
pub(crate) fn fetch_proposals_by_owner(module_tag_scale: &[u8]) -> Result<Vec<u128>, String> {
    fetch_proposal_ids_by_index("ProposalsByOwner", module_tag_scale)
}

#[cfg(test)]
#[path = "../tests/display.rs"]
mod format_summary_tests;

pub(crate) fn read_plan_at(
    pid: u128,
    at: &str,
) -> Result<engingcore::types::VotePlanOf<sp_runtime::AccountId32>, String> {
    let key = nodeclient::query::map_key("VotingEngine", "ProposalVotePlans", &pid.to_le_bytes());
    let value = nodeclient::query::fetch_storage_at(&key, at)?.ok_or("原投票计划缺失")?;
    engingcore::types::VotePlanOf::decode_all(&mut &decode_hex_storage(&value)?[..])
        .map_err(|e| format!("原投票计划损坏: {e}"))
}

pub(crate) fn load_frozen_vote_view(pid: u128) -> Result<crate::types::FrozenVoteView, String> {
    let at = nodeclient::query::fetch_finalized_head()?;
    load_frozen_vote_view_at(pid, &at)
}
pub(crate) fn load_frozen_vote_view_at(
    pid: u128,
    at: &str,
) -> Result<crate::types::FrozenVoteView, String> {
    let header = nodeclient::signing::rpc_post("chain_getHeader", serde_json::json!([at]))?;
    let height = u64::from_str_radix(
        header["number"]
            .as_str()
            .ok_or("最终块高度缺失")?
            .trim_start_matches("0x"),
        16,
    )
    .map_err(|_| "最终块高度损坏")?;
    let genesis = format!(
        "0x{}",
        hex::encode(nodeclient::signing::fetch_genesis_hash()?)
    );
    let view = read_frozen_vote_view(pid, at, height, &genesis, |key| {
        nodeclient::query::fetch_storage_at(key, at)
    })?;
    crate::vote::observe_completed_contexts(pid, &view)?;
    Ok(view)
}

/// 可注入只读同块读取器，测试不得连接或写运行链。
pub(crate) fn read_frozen_vote_view(
    pid: u128,
    at: &str,
    height: u64,
    genesis: &str,
    mut read: impl FnMut(&str) -> Result<Option<String>, String>,
) -> Result<crate::types::FrozenVoteView, String> {
    use crate::types::{FrozenRoleSnapshot, FrozenVoteView, FrozenVoter};
    let proposal_key = nodeclient::query::map_key("VotingEngine", "Proposals", &pid.to_le_bytes());
    let raw = read(&proposal_key)?.ok_or("原提案不存在")?;
    let proposal = engingcore::types::Proposal::<u32, sp_runtime::AccountId32>::decode_all(
        &mut &decode_hex_storage(&raw)?[..],
    )
    .map_err(|e| format!("原提案损坏: {e}"))?;
    let key = nodeclient::query::map_key("VotingEngine", "ProposalVotePlans", &pid.to_le_bytes());
    let raw = read(&key)?.ok_or("原投票计划缺失")?;
    let plan = engingcore::types::VotePlanOf::<sp_runtime::AccountId32>::decode_all(
        &mut &decode_hex_storage(&raw)?[..],
    )
    .map_err(|e| format!("原投票计划损坏: {e}"))?;
    let data_key = nodeclient::query::map_key("VotingEngine", "ProposalData", &pid.to_le_bytes());
    let data = read(&data_key)?
        .map(|v| {
            Vec::<u8>::decode_all(&mut &decode_hex_storage(&v)?[..]).map_err(|e| e.to_string())
        })
        .transpose()?;
    let mut scope_error = if data
        .as_ref()
        .is_some_and(|v| v.starts_with(TAG_PUBLIC_MANAGE) || v.starts_with(TAG_PRIVATE_MANAGE))
    {
        Some("机构管理提案不属于节点原治理投票范围".to_string())
    } else {
        None
    };
    let mut roles = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for subject in plan.voter_subjects {
        let AuthorizationSubject::Institution(ref role) = subject else {
            continue;
        };
        if !seen.insert(subject.encode()) {
            return Err("原投票主体重复".into());
        }
        let cid = String::from_utf8(role.cid_number.to_vec()).map_err(|_| "机构身份CID号损坏")?;
        let code = String::from_utf8(role.role_code.to_vec()).map_err(|_| "原岗位标识损坏")?;
        if !primitives::cid::code::institution_code_from_cid_number(&cid)
            .is_some_and(|c| primitives::cid::code::is_fixed_governance_code(&c))
        {
            scope_error = Some("不属于节点原治理机构投票范围".into());
        }
        let key = nodeclient::query::double_map_key(
            "VotingEngine",
            "RoleVoters",
            &pid.to_le_bytes(),
            &role.encode(),
        );
        let mut voters = Vec::new();
        let mut error = None;
        if let Some(raw) = read(&key)? {
            let persons = Vec::<Vec<u8>>::decode_all(&mut decode_hex_storage(&raw)?.as_slice())
                .map_err(|e| format!("冻结人员CID损坏:{e}"))?;
            if persons.is_empty()
                || persons.len() > 1989
                || persons.windows(2).any(|pair| pair[0] >= pair[1])
            {
                return Err("冻结人员集合为空、重复、未排序或超限".into());
            }
            for person in persons {
                if !nodeinstitution::storage::person_allowed(
                    &person,
                    nodeinstitution::storage::EntityPallet::Public,
                ) {
                    return Err("公权冻结人员CID无效".into());
                }
                let person = String::from_utf8(person).map_err(|_| "人员CID损坏")?;
                let binding = nodeinstitution::storage::person_binding(&person, &mut read)?;
                let key = crate::tally::institution_vote_key(
                    pid,
                    proposal.kind,
                    &cid,
                    &code,
                    person.as_bytes(),
                )?;
                let (vote, vote_error) = match read(&key)
                    .and_then(|raw| crate::tally::decode_option_vote(raw.as_deref()))
                {
                    Ok(v) => (v, None),
                    Err(e) => (None, Some(e)),
                };
                voters.push(FrozenVoter {
                    person_cid: Some(person),
                    account_id: binding.as_ref().map(|b| b.0.clone()),
                    binding_revision: binding.map(|b| b.1),
                    vote,
                    vote_error,
                });
            }
        } else if proposal.status != 0 {
            // 仅终态历史读取原始钱包布局，绝不将其转换为人员CID或新投票资格。
            let key = nodeclient::query::double_map_key(
                "VotingEngine",
                "VoterSnapshot",
                &pid.to_le_bytes(),
                &subject.encode(),
            );
            if let Some(raw) = read(&key)? {
                let wallets =
                    Vec::<[u8; 32]>::decode_all(&mut decode_hex_storage(&raw)?.as_slice())
                        .map_err(|e| format!("历史原钱包快照损坏:{e}"))?;
                if wallets.is_empty()
                    || wallets.len() > 1989
                    || wallets
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        != wallets.len()
                {
                    return Err("历史钱包快照无效".into());
                }
                for wallet in wallets {
                    let mut ticket = if proposal.kind == 0 {
                        vec![1]
                    } else {
                        Vec::new()
                    };
                    ticket.extend((role.clone(), wallet).encode());
                    let (pallet, item) = if proposal.kind == 0 {
                        ("InternalVote", "InternalVotesByTicket")
                    } else {
                        ("JointVote", "JointVotesByTicket")
                    };
                    let key = nodeclient::query::double_map_key(
                        pallet,
                        item,
                        &pid.to_le_bytes(),
                        &ticket,
                    );
                    let (vote, vote_error) = match read(&key)
                        .and_then(|raw| crate::tally::decode_option_vote(raw.as_deref()))
                    {
                        Ok(v) => (v, None),
                        Err(e) => (None, Some(e)),
                    };
                    voters.push(FrozenVoter {
                        person_cid: None,
                        account_id: Some(format!("0x{}", hex::encode(wallet))),
                        binding_revision: None,
                        vote,
                        vote_error,
                    });
                }
            } else {
                error = Some("历史快照已按原规则清理或缺失".into())
            }
        } else {
            error = Some("冻结人员快照缺失，不能取得机构投票资格".into())
        }
        roles.push(FrozenRoleSnapshot {
            cid_number: cid,
            role_code: code,
            error,
            voters,
        });
    }
    Ok(FrozenVoteView {
        proposal_id: pid.to_string(),
        genesis_hash: genesis.into(),
        block_hash: at.into(),
        block_number: height.to_string(),
        end_block: proposal.end.to_string(),
        kind: proposal.kind,
        stage: proposal.stage,
        status: proposal.status,
        actor_cid_number: proposal
            .actor_cid_number
            .map(|c| String::from_utf8_lossy(&c).into_owned()),
        scope_error,
        roles,
    })
}

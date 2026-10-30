use nodeupgrade::types::RuntimeUpgradeDetail;

use noderesolutionissue::types::ResolutionIssuanceDetail;

use nodedestroy::types::ResolutionDestroyDetail;

use nodeclient::codec::decode_hex_storage;

use nodeupgrade::decode::decode_runtime_upgrade_action;

use noderesolutionissue::decode::decode_resolution_issuance_action;

use nodedestroy::decode::decode_resolution_destroy_action;

use nodeclient::codec::decode_u64_vec;

use nodeclient::codec::read_compact_u32;

use nodeclient::codec::decode_cid_number;

use nodeclient::codec::decode_subject_cid_numbers;

use primitives::cid::code::InstitutionCode;

use codec::Decode;

use crate::display::fetch_proposal_display;
use crate::display::format_proposal_id;
use crate::display::kind_label;
use crate::display::resolve_cid_full_name_by_subjects;
use crate::display::stage_label;
use crate::display::status_label;
use crate::display::ProposalDisplayInfo;
use crate::tally::fetch_internal_tally;
use crate::tally::fetch_joint_tally;
use crate::tally::fetch_referendum_tally;
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
pub(crate) fn fetch_next_proposal_id() -> Result<u64, String> {
    let key = nodeclient::query::value_key("VotingEngine", "NextProposalId");
    // (ADR-017):业务读取统一经 chain_query 钉 finalized,禁止 best。
    match nodeclient::query::fetch_finalized_storage(&key)? {
        None => Ok(0),
        Some(hex_data) => {
            let data = decode_hex_storage(&hex_data)?;
            if data.len() < 8 {
                return Ok(0);
            }
            Ok(u64::from_le_bytes(
                data[..8]
                    .try_into()
                    .map_err(|_| "SCALE 数据长度不足".to_string())?,
            ))
        }
    }
}

/// 检查提案是否为机构管理提案（创建/关闭机构多签账户），这类提案不在治理列表中显示。
/// 机构管理已拆公权/私权两 pallet,命中 `pub-mgmt` 或 `pri-mgmt` 任一前缀即认定。
fn is_institution_manage_proposal(proposal_id: u64) -> bool {
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
pub(crate) fn fetch_proposal_page(start_id: u64, count: u32) -> Result<ProposalPageResult, String> {
    let mut items = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let min_id = start_id.saturating_sub(count as u64);
    let mut id = start_id;

    while id > min_id {
        match fetch_proposal_meta(id) {
            Ok(Some(meta)) => {
                // 多签管理提案(创建/关闭多签账户)不在治理列表中显示。
                if is_institution_manage_proposal(id) {
                    if id == 0 {
                        break;
                    }
                    id -= 1;
                    continue;
                }
                // 协议升级摘要只保存展示字段，真实状态统一读取 votebase。
                let display = match fetch_proposal_display(id, &meta) {
                    Ok(v) => v,
                    Err(_) => ProposalDisplayInfo {
                        summary: "(详情查询失败)".to_string(),
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
                });
            }
            Ok(None) => {} // 提案不存在,跳过
            Err(e) => {
                warnings.push(format!("查询提案 {id} 失败: {e}"));
            }
        }
        if id == 0 {
            break;
        }
        id -= 1;
    }

    // 双层 ID：主键单调，下界为 0。
    let has_more = min_id > 0;

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
pub(crate) fn fetch_proposal_full(proposal_id: u64) -> Result<ProposalFullInfo, String> {
    let meta =
        fetch_proposal_meta(proposal_id)?.ok_or_else(|| format!("提案 {proposal_id} 不存在"))?;

    // 业务动作:一次性解析,按变体分发到 7 个 *_detail 字段。
    let action = resolve_proposal_action(proposal_id, &meta)?;
    let (
        business_details,
        runtime_upgrade_detail,
        resolution_issuance_detail,
        resolution_destroy_detail,
        fee_rate_detail,
    ) = split_action_into_details(action);

    // 根据提案阶段查询对应的投票计数。
    let internal_tally = if meta.kind == 0 || meta.stage == 0 {
        fetch_internal_tally(proposal_id).ok()
    } else {
        None
    };

    let joint_tally = if meta.kind == 1 && meta.stage >= 1 {
        fetch_joint_tally(proposal_id).ok()
    } else {
        None
    };

    let referendum_tally = if meta.kind == 1 && meta.stage >= 2 {
        fetch_referendum_tally(proposal_id).ok()
    } else {
        None
    };

    let cid_full_name = resolve_cid_full_name_by_subjects(&meta.subject_cid_numbers);
    let voter_role_subjects = fetch_voter_role_subjects(proposal_id)?;

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
        cid_full_name,
    })
}

fn fetch_voter_role_subjects(proposal_id: u64) -> Result<Vec<VoterRoleSubject>, String> {
    let key = nodeclient::query::map_key("VotingEngine", "VotePlans", &proposal_id.to_le_bytes());
    let Some(hex_value) = nodeclient::query::fetch_finalized_storage(&key)? else {
        return Err(format!("提案 {proposal_id} 缺少 VotingEngine::VotePlans"));
    };
    let bytes = hex::decode(hex_value.trim_start_matches("0x"))
        .map_err(|error| format!("VotePlans hex 解码失败: {error}"))?;
    let mut input = &bytes[..];
    let plan = votebase::types::VotePlanOf::<sp_runtime::AccountId32>::decode(&mut input)
        .map_err(|error| format!("VotePlans SCALE 解码失败: {error}"))?;
    if !input.is_empty() {
        return Err("VotePlans SCALE 存在尾随字节".into());
    }
    let mut subjects = Vec::new();
    for subject in plan.voter_subjects {
        if let AuthorizationSubject::Institution(role) = subject {
            subjects.push(VoterRoleSubject {
                cid_number: String::from_utf8(role.cid_number.into_inner())
                    .map_err(|_| "VotePlans cid_number 不是 UTF-8".to_string())?,
                role_code: String::from_utf8(role.role_code.into_inner())
                    .map_err(|_| "VotePlans role_code 不是 UTF-8".to_string())?,
            });
        }
    }
    Ok(subjects)
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
    start_id: u64,
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
    let take_ids: Vec<u64> = ids
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
pub(crate) fn fetch_active_proposal_ids(cid_number: &str) -> Result<Vec<u64>, String> {
    let subject_key = proposal_subject_institution_cid_key(cid_number)?;
    let key = nodeclient::query::map_key("VotingEngine", "ActiveProposalsBySubject", &subject_key);
    // (ADR-017):活跃提案索引按 finalized 口径读取,避免 best 漂移漏列。
    match nodeclient::query::fetch_finalized_storage(&key)? {
        None => Ok(Vec::new()),
        Some(hex_data) => {
            let data = decode_hex_storage(&hex_data)?;
            decode_u64_vec(&data)
        }
    }
}

// ──── 内部查询 ────

pub(crate) fn fetch_proposal_meta(proposal_id: u64) -> Result<Option<ProposalMeta>, String> {
    let key = nodeclient::query::map_key("VotingEngine", "Proposals", &proposal_id.to_le_bytes());
    // (ADR-017):提案元数据(含 status)按 finalized 口径读取,禁止 best。
    match nodeclient::query::fetch_finalized_storage(&key)? {
        None => Ok(None),
        Some(hex_data) => {
            let data = decode_hex_storage(&hex_data)?;
            Ok(decode_proposal_meta(proposal_id, &data))
        }
    }
}

fn fetch_proposal_data_raw(proposal_id: u64) -> Result<Option<Vec<u8>>, String> {
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
    proposal_id: u64,
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

fn decode_proposal_meta(proposal_id: u64, data: &[u8]) -> Option<ProposalMeta> {
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

/// 查询 `VotingEngine::ProposalDisplayId[id]` → `ProposalDisplayMeta { year:u16, seq_in_year:u32 }`。
/// 6 字节 SCALE:u16 LE + u32 LE。
pub(crate) fn fetch_proposal_display_id(
    proposal_id: u64,
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
    if raw.len() < 6 {
        return Ok(None);
    }
    let year = u16::from_le_bytes([raw[0], raw[1]]);
    let seq_in_year = u32::from_le_bytes([raw[2], raw[3], raw[4], raw[5]]);
    Ok(Some(ProposalDisplayMeta { year, seq_in_year }))
}

/// 通用反向索引迭代器:列举 `StorageDoubleMap<_, Twox64Concat, K1, Twox64Concat, u64, ()>`
/// 在指定 K1 下的所有 proposal_id。每条 key 末 8 字节 = u64 LE = proposal_id。
fn fetch_proposal_ids_by_index(storage_name: &str, key1: &[u8]) -> Result<Vec<u64>, String> {
    let prefix = nodeclient::query::twox64_concat_prefix("VotingEngine", storage_name, key1);
    // (ADR-017):反向索引列举按 finalized 口径,best 漂移会列出半新半旧 key 集。
    let keys = nodeclient::query::fetch_finalized_keys_paged(&prefix, 1000, None)?;
    let mut ids = Vec::with_capacity(keys.len());
    for s in &keys {
        let bytes = match hex::decode(s.trim_start_matches("0x")) {
            Ok(b) => b,
            Err(_) => continue,
        };
        if bytes.len() < 8 {
            continue;
        }
        let mut tail = [0u8; 8];
        tail.copy_from_slice(&bytes[bytes.len() - 8..]);
        ids.push(u64::from_le_bytes(tail));
    }
    Ok(ids)
}

/// 反向索引:`ProposalsByCode[institution_code]` → 该机构码下所有 proposal_id。
///
/// 链上第一腿 key 已从 `org: u8`(1 字节)改为 `InstitutionCode`([u8;4]),
/// `institution_code` 入参是 4 字符机构码字符串(如 "NRC"/"CGOV")。
pub(crate) fn fetch_proposals_by_institution_code(
    institution_code: &str,
) -> Result<Vec<u64>, String> {
    let code = primitives::cid::code::code_bytes(institution_code.trim());
    fetch_proposal_ids_by_index("ProposalsByCode", &code)
}

/// 反向索引:`ProposalsByCid[cid_number]` → 本机构主体所有关联 proposal_id。
pub(crate) fn fetch_proposals_by_cid(cid_number: &str) -> Result<Vec<u64>, String> {
    let cid_key = cid_number_key(cid_number)?;
    fetch_proposal_ids_by_index("ProposalsByCid", &cid_key)
}

/// 反向索引:`ProposalsByOwner[module_tag]` → 该业务模块所有 proposal_id。
/// `module_tag` 入参为 BoundedVec<u8> 的 SCALE 编码体(Compact<len> + bytes)。
pub(crate) fn fetch_proposals_by_owner(module_tag_scale: &[u8]) -> Result<Vec<u64>, String> {
    fetch_proposal_ids_by_index("ProposalsByOwner", module_tag_scale)
}

#[cfg(test)]
#[path = "../tests/display.rs"]
mod format_summary_tests;

//! 本机构通用资料与管理员读取，保留既有链上鉴权及展示字段。
use axum::{extract::State, http::HeaderMap, http::StatusCode, response::IntoResponse, Json};
use std::collections::BTreeMap;

use crate::authentication::repo;
use crate::chain_runtime;
use crate::crypto::pubkey::same_account_id;
use crate::institutions::subjects::http::resolve_created_by;
use crate::institutions::subjects::model::InstitutionDetailOutput;
use crate::*;

fn balance_lookup_key(account_id: &str) -> String {
    crate::crypto::pubkey::normalize_account_id(account_id).unwrap_or_default()
}

pub fn balance_fen(
    balances: &BTreeMap<String, Option<String>>,
    account_id: &str,
) -> Option<String> {
    balances
        .get(balance_lookup_key(account_id).as_str())
        .cloned()
        .flatten()
}

/// 普通机构只读“本机构管理员”列表。
///
/// 权威集合仍来自链上 Active 管理员集合;本地 admins 表只补展示姓名,不能决定准入。
pub(crate) async fn list_own_institution_admins(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let capabilities =
        crate::modules::capability::capabilities_for(&state.modules, &ctx.institution_code);
    if !capabilities.view_own_admins {
        return api_error(StatusCode::FORBIDDEN, 1003, "permission denied");
    }
    let binding = match repo::active_node_binding(&state.db) {
        Ok(Some(binding)) => binding,
        Ok(None) => return api_error(StatusCode::FORBIDDEN, 2002, "not an on-chain admin"),
        Err(err) => {
            let message = format!("query node binding failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };
    if binding.institution_code != ctx.institution_code {
        return api_error(StatusCode::FORBIDDEN, 1003, "permission denied");
    }
    let identity = match chain_runtime::identity_from_binding_parts(
        &binding.institution_code,
        Some(binding.institution_cid_number.as_str()),
    ) {
        Ok(identity) => identity,
        Err(err) => {
            let message = format!("node binding invalid: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };
    let chain_assignments =
        match crate::institutions::admins::chain_roles::fetch_active_assignments_onchain(&identity)
            .await
        {
            Ok(Some(assignments)) => assignments,
            Ok(None) => return api_error(StatusCode::FORBIDDEN, 2002, "not an on-chain admin"),
            Err(err) => {
                tracing::warn!(error = %err, "chain unreachable listing own institution admins");
                return api_error(StatusCode::BAD_GATEWAY, 5002, "chain unreachable");
            }
        };
    // 列表展示也做一次链上 active 复查,避免后台清退窗口内的失效管理员继续读取。
    if !chain_assignments
        .iter()
        .any(|assignment| same_account_id(assignment.account_id.as_str(), ctx.account_id.as_str()))
    {
        return api_error(StatusCode::FORBIDDEN, 2002, "not an on-chain admin");
    }
    let actor_account_id = ctx.account_id.clone();
    let institution_code = ctx.institution_code.clone();
    let cid_short_name = ctx.cid_short_name.clone();
    let balance_account_ids = chain_assignments
        .iter()
        .map(|assignment| assignment.account_id.clone())
        .collect::<Vec<_>>();
    let balance_by_account_id = match chain_runtime::fetch_account_balances_onchain(
        &balance_account_ids,
    )
    .await
    {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(error = %err, "chain balance unavailable listing own institution admins");
            BTreeMap::new()
        }
    };
    let result = {
        let mut rows = Vec::with_capacity(chain_assignments.len());
        for assignment in chain_assignments {
            let is_self =
                same_account_id(assignment.account_id.as_str(), actor_account_id.as_str());
            let balance = balance_fen(&balance_by_account_id, assignment.account_id.as_str());
            rows.push(OwnInstitutionAdminRow {
                account_id: assignment.account_id,
                family_name: assignment.family_name,
                given_name: assignment.given_name,
                role_code: assignment.role_code,
                role_name: assignment.role_name,
                term_required: assignment.term_required,
                term_start: assignment.term_start,
                term_end: assignment.term_end,
                assignment_source: assignment.assignment_source,
                assignment_source_label: assignment.assignment_source_label,
                assignment_source_ref: assignment.assignment_source_ref,
                balance_fen: balance,
                is_self,
            });
        }
        Ok::<_, String>(OwnInstitutionAdminListOutput {
            institution_code,
            cid_short_name,
            rows,
        })
    };
    let data = match result {
        Ok(v) => v,
        Err(err) => {
            let message = format!("query own institution admins failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data,
    })
    .into_response()
}

/// 当前登录机构详情。
///
/// 只读取本节点 active binding 对应的机构 CID,不允许前端传 cid_number,避免把“本机构信息”
/// 变成任意机构详情读取入口。管理员资格仍由登录守卫和链上 active admins 决定。
pub(crate) async fn get_own_institution(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let binding = match repo::active_node_binding(&state.db) {
        Ok(Some(binding)) => binding,
        Ok(None) => return api_error(StatusCode::FORBIDDEN, 2002, "not an on-chain admin"),
        Err(err) => {
            let message = format!("query node binding failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };
    if binding.institution_code != ctx.institution_code {
        return api_error(StatusCode::FORBIDDEN, 1003, "permission denied");
    }
    if binding.institution_cid_number != ctx.institution_cid_number {
        return api_error(StatusCode::FORBIDDEN, 1003, "permission denied");
    }
    if let Err(error) =
        crate::institutions::catalog::projection::refresh_bound_institution(&state.db).await
    {
        tracing::warn!(error = %error, "bound institution projection unavailable");
        return api_error(StatusCode::BAD_GATEWAY, 5002, "chain unreachable");
    }
    let cid_number = binding.institution_cid_number.trim().to_string();
    if cid_number.is_empty() {
        return api_error(
            StatusCode::CONFLICT,
            1007,
            "binding institution_cid_number is required",
        );
    }
    let Some((inst, _accounts)) = (match state.db.get_institution_with_accounts(cid_number.as_str())
    {
        Ok(v) => v,
        Err(err) => {
            let message = format!("query own institution failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    }) else {
        return api_error(StatusCode::NOT_FOUND, 1004, "institution not found");
    };
    let accounts = match crate::institutions::accounts::handler::detail_accounts(
        &inst.institution_code,
        &inst.cid_number,
    )
    .await
    {
        Ok(accounts) => accounts,
        Err(error) => {
            tracing::warn!(error = %error, "institution detail chain accounts unavailable");
            return api_error(StatusCode::BAD_GATEWAY, 5002, "chain unreachable");
        }
    };
    let (creator_family_name, creator_given_name, creator_institution_code) =
        resolve_created_by(&state, inst.creator_account_id.as_deref().unwrap_or(""));
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: InstitutionDetailOutput {
            institution: inst,
            accounts,
            creator_family_name,
            creator_given_name,
            creator_institution_code,
        },
    })
    .into_response()
}

/// 绑定机构及公共机构字段的原投影实现；仅调整归属，不改CID地域规则。
pub mod projection {
    use crate::chain_runtime::{institution_lookup_at, is_tier1_registry, OnChainInstitution};
    use crate::codes::InstitutionCategory;
    use crate::institutions::subjects::model::LegalRepresentative;
    use crate::institutions::subjects::Institution;
    use crate::Db;
    use chrono::{DateTime, Utc};
    use primitives::cid::code::is_private_legal_code;
    use primitives::cid::number::{cid_scope_codes, parse_cid_number_parts};
    /// 链上机构数据(公权/私权;由 chain_read 层从 Institutions 读出)。
    #[derive(Debug, Clone)]
    pub struct ChainInstitution {
        pub cid_number: String,
        /// CID 省码(= 归属省)。
        pub province_code: String,
        /// CID 市码(= 归属市)。机构按 CID 市码归属,不依赖 residence。
        pub city_code: String,
        pub town_code: String,
        pub cid_full_name: String,
        pub cid_short_name: String,
        pub institution_code: String,
        pub profit: bool,
        pub is_private: bool,
        pub legal_representative: Option<LegalRepresentative>,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    /// 本节点作用域，沿用原省代码及省内市编号。
    pub struct NodeScope {
        pub province_code: String,
        pub city_code: String,
    }
    /// 链上字段优先；缺失时保留本地正本，其后才为空串。
    pub fn chain_or_local_str(chain: &Option<String>, local: Option<&str>) -> String {
        chain
            .clone()
            .or_else(|| local.map(str::to_string))
            .unwrap_or_default()
    }
    /// 从机构 CID 唯一解析 (province_code, city_code)。复用 primitives 权威单源
    /// `cid_scope_codes`;人主体 CID(去地域化、R5 不载省市)传入即 fail-closed 返回 Err。
    /// 本函数仅应收机构 CID(节点自身机构 / 公私权机构投影)。
    pub fn r5_province_city(cid_number: &str) -> Result<(String, String), String> {
        let (province, city) = cid_scope_codes(cid_number.as_bytes())
            .map_err(|e| format!("cid {cid_number} scope invalid: {e}"))?;
        Ok((
            String::from_utf8_lossy(&province).into_owned(),
            String::from_utf8_lossy(&city).into_owned(),
        ))
    }
    /// 解析本节点作用域:`Ok(None)` = 未绑定机构;`Ok(Some((is_federal, scope)))`。
    /// 城市/省注册局 scope = 其自身机构 CID 的 (省,市);联邦(Tier1)只用 is_federal 标记。
    pub fn resolve_node_scope(db: &Db) -> Result<Option<(bool, NodeScope)>, String> {
        let Some(binding) = crate::authentication::repo::active_node_binding(db)? else {
            return Ok(None);
        };
        let is_federal = is_tier1_registry(binding.institution_code.as_str());
        let (province_code, city_code) = r5_province_city(binding.institution_cid_number.as_str())?;
        Ok(Some((
            is_federal,
            NodeScope {
                province_code,
                city_code,
            },
        )))
    }
    pub fn chain_institution_from_lookup(
        cid_number: &str,
        oci: OnChainInstitution,
    ) -> Result<ChainInstitution, String> {
        let parts = parse_cid_number_parts(cid_number)
            .map_err(|e| format!("institution cid {cid_number} invalid: {e}"))?;
        if oci.institution_code != parts.institution {
            return Err("institution storage code does not match CID".into());
        }
        let (province_code, city_code) = r5_province_city(cid_number)?;
        let is_private = is_private_legal_code(&parts.institution);
        let legal_representative = oci
            .legal_representative
            .map(|lr| -> Result<LegalRepresentative, String> {
                Ok(LegalRepresentative {
                    family_name: String::from_utf8(lr.family_name)
                        .map_err(|_| "institution representative family name is not UTF-8")?,
                    given_name: String::from_utf8(lr.given_name)
                        .map_err(|_| "institution representative given name is not UTF-8")?,
                    cid_number: String::from_utf8(lr.cid_number)
                        .map_err(|_| "institution representative CID is not UTF-8")?,
                    account_id: format!("0x{}", hex::encode(lr.account_id)),
                })
            })
            .transpose()?;
        Ok(ChainInstitution {
            cid_number: cid_number.to_string(),
            province_code,
            city_code,
            town_code: String::from_utf8(oci.town_code)
                .map_err(|_| "institution town code is not UTF-8")?,
            cid_full_name: String::from_utf8(oci.cid_full_name)
                .map_err(|_| "institution full name is not UTF-8")?,
            cid_short_name: String::from_utf8(oci.cid_short_name)
                .map_err(|_| "institution short name is not UTF-8")?,
            institution_code: parts.institution_code_text,
            profit: parts.profit,
            is_private,
            legal_representative,
        })
    }
    /// 计算机构投影要写回的最终行;`None` = 不在本作用域,跳过。
    ///
    /// - **作用域**:机构按 CID 的 (省,市) 归属;不匹配本节点则跳过。
    /// - **链上来源列**(投影权威):全称/简称、省/市/镇码、机构码、盈利位、法人标记、法定代表人。
    /// - **链下正本列**(绝不覆盖,从 existing 保留):法定代表人证件照、机构业务分类(private_type/
    ///   partnership_kind/education_type,由市补录)、创建人、创建时间。
    pub fn merge_institution_record(
        chain: &ChainInstitution,
        existing: Option<&Institution>,
        scope: &NodeScope,
        now: DateTime<Utc>,
    ) -> Option<Institution> {
        if chain.province_code != scope.province_code || chain.city_code != scope.city_code {
            return None;
        }
        Some(Institution {
            status: "ACTIVE".to_string(),
            chain_status: Some("ACTIVE_ON_CHAIN".to_string()),
            subject_property: Institution::subject_property_for_code(&chain.institution_code)
                .ok()?,
            cid_number: chain.cid_number.clone(),
            cid_full_name: Some(chain.cid_full_name.clone()),
            cid_short_name: Some(chain.cid_short_name.clone()),
            category: if chain.is_private {
                InstitutionCategory::PrivateInstitution
            } else {
                InstitutionCategory::GovInstitution
            },
            p1: if chain.profit { "1" } else { "0" }.to_string(),
            // 行政区名字不入库(china.sqlite 单源,读时派生),只存代码。
            province_name: String::new(),
            city_name: String::new(),
            town_name: String::new(),
            province_code: chain.province_code.clone(),
            city_code: chain.city_code.clone(),
            town_code: chain.town_code.clone(),
            institution_code: chain.institution_code.clone(),
            // ── 链下正本 / 市补录业务分类:保留 existing ──
            education_type: existing.and_then(|e| e.education_type.clone()),
            // private_type 优先保留本地既有(市补录可能更细),纯链上投影(existing=None)按机构码
            // 确定性派生,否则私权列表 SQL 的 `private_type IS NOT NULL` 会漏掉纯投影的私权机构。
            private_type: existing.and_then(|e| e.private_type.clone()).or_else(|| {
                crate::institutions::model::private_type_code_from_institution_code(
                    &chain.institution_code,
                )
                .map(str::to_string)
            }),
            partnership_kind: existing.and_then(|e| e.partnership_kind.clone()),
            has_legal_personality: if chain.is_private {
                Some(true) // 私权法人
            } else {
                existing.and_then(|e| e.has_legal_personality)
            },
            parent_cid_number: existing.and_then(|e| e.parent_cid_number.clone()),
            // ── 链上来源:法定代表人(姓名/CID/账户)投影权威 ──
            legal_representative: chain.legal_representative.clone(),
            // ── 链下正本:证件照保留 existing ──
            photo_path: existing.and_then(|e| e.photo_path.clone()),
            photo_name: existing.and_then(|e| e.photo_name.clone()),
            photo_mime: existing.and_then(|e| e.photo_mime.clone()),
            photo_size: existing.and_then(|e| e.photo_size),
            creator_account_id: existing.and_then(|e| e.creator_account_id.clone()),
            created_at: existing.map(|e| e.created_at).unwrap_or(now),
        })
    }
    pub fn apply_institution_projection_conn(
        conn: &mut impl postgres::GenericClient,
        chain: &ChainInstitution,
        scope: &NodeScope,
    ) -> Result<bool, String> {
        // 照片更新也锁定 subjects 行；锁后再读取并保留链下字段。
        conn.query_opt(
            "SELECT cid_number FROM subject WHERE cid_number=$1 FOR UPDATE",
            &[&chain.cid_number],
        )
        .map_err(|e| format!("lock institution projection failed: {e}"))?;
        let existing =
            Db::get_institution_with_accounts_conn(conn, &chain.cid_number)?.map(|(inst, _)| inst);
        let Some(row) = merge_institution_record(chain, existing.as_ref(), scope, Utc::now())
        else {
            return Ok(false);
        };
        Db::upsert_target_subject_rows(conn, &row)?;
        Ok(true)
    }
    /// 绑定机构不局限于创世基金会或 CREG：准确 CID 的机构及账户在固定 finalized 区块投影。
    /// 网络读在事务外；落库时复核原绑定并保留照片、创建人等链下正本。
    pub(crate) async fn refresh_bound_institution(db: &Db) -> Result<bool, String> {
        let Some(binding) = crate::authentication::repo::active_node_binding(db)? else {
            return Ok(false);
        };
        let finalized = crate::chain_runtime::FinalizedChainView::connect().await?;
        let hash = format!("{:#x}", finalized.block_hash);
        let genesis = format!("{:#x}", finalized.client.genesis_hash());
        let detail = institution_lookup_at(
            &binding.institution_cid_number,
            Some(finalized.block_hash.0),
        )
        .await?;
        let accounts = if detail.is_some() {
            let code = primitives::cid::code::institution_code_from_str(&binding.institution_code)
                .ok_or("invalid bound institution code")?;
            crate::chain_runtime::institution_accounts_lookup_at(
                &code,
                &binding.institution_cid_number,
                &finalized,
            )
            .await?
        } else {
            Vec::new()
        };
        let chain = detail
            .map(|detail| chain_institution_from_lookup(&binding.institution_cid_number, detail))
            .transpose()?;
        db.with_client(|conn| {
        let mut tx = conn.transaction().map_err(|e| format!("begin bound institution projection failed: {e}"))?;
        crate::authentication::repo::lock_node_binding_conn(&mut tx)?;
        let active = crate::authentication::repo::get_active_node_binding_conn(&mut tx)?;
        if !active.is_some_and(|active| active.binding_id == binding.binding_id && active.institution_cid_number == binding.institution_cid_number) { return Ok(false); }
        let key = format!("bound-institution:{}", binding.institution_cid_number);
        if let Some(row) = tx.query_opt("SELECT genesis_hash,block_number FROM projection WHERE projection_key=$1", &[&key]).map_err(|e| e.to_string())? {
            if row.get::<_, String>(0) != genesis { return Err("bound institution projection genesis mismatch".into()); }
            if row.get::<_, Option<i64>>(1).is_some_and(|number| number > i64::from(finalized.block_number)) { return Ok(false); }
        }
        if let Some(chain) = &chain {
            let scope = NodeScope { province_code: chain.province_code.clone(), city_code: chain.city_code.clone() };
            apply_institution_projection_conn(&mut tx, chain, &scope)?;
            tx.execute("UPDATE government SET source_kind='CHAIN' WHERE cid_number=$1", &[&binding.institution_cid_number]).map_err(|e| e.to_string())?;
        } else {
            tx.execute("UPDATE subject SET state_code='REVOKED',chain_status='REVOKED_ON_CHAIN',updated_at=now() WHERE cid_number=$1", &[&binding.institution_cid_number]).map_err(|e| e.to_string())?;
        }
        let mut names = Vec::with_capacity(accounts.len());
        for account in &accounts {
            if account.cid_number.as_slice() != binding.institution_cid_number.as_bytes() { return Err("bound institution account CID mismatch".into()); }
            let name = String::from_utf8(account.account_name.clone()).map_err(|_| "institution account name is not UTF-8")?;
            let (province, city) = r5_province_city(&binding.institution_cid_number)?;
            let account_id = format!("0x{}", hex::encode(account.account_id));
            tx.execute("INSERT INTO account(cid_number,province_code,city_code,account_name,account_id,created_at) VALUES($1,$2,$3,$4,$5,now()) ON CONFLICT(province_code,cid_number,account_name) DO UPDATE SET account_id=EXCLUDED.account_id,city_code=EXCLUDED.city_code",
                &[&binding.institution_cid_number,&province,&city,&name,&account_id]).map_err(|e| e.to_string())?;
            names.push(name);
        }
        // 仅清理由链同步拥有的账户查询行；机构档案与附件均保留。
        tx.execute("DELETE FROM account WHERE cid_number=$1 AND NOT(account_name=ANY($2))", &[&binding.institution_cid_number,&names]).map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO projection(projection_key,genesis_hash,block_hash,block_number,item_count,account_count,state_code,synced_at) VALUES($1,$2,$3,$4,$5,$6,'OK',now()) ON CONFLICT(projection_key) DO UPDATE SET genesis_hash=EXCLUDED.genesis_hash,block_hash=EXCLUDED.block_hash,block_number=EXCLUDED.block_number,item_count=EXCLUDED.item_count,account_count=EXCLUDED.account_count,state_code='OK',synced_at=now()",
            &[&key,&genesis,&hash,&i64::from(finalized.block_number),&(if chain.is_some() { 1i64 } else { 0i64 }),&i64::try_from(accounts.len()).map_err(|_| "account count exceeds i64")?]).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| format!("commit bound institution projection failed: {e}"))?;
        Ok(true)
    })
    }
}

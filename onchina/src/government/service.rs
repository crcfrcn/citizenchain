//! 公权机构链上投影服务。
//!
//! 公权机构唯一真源是链上 `PublicManage::Institutions` 与
//! `PublicManage::InstitutionAccounts`。OnChina PostgreSQL 只保存查询缓存:
//! 启动或显式同步时全量读取链上 storage,只把链上存在的机构投影到本地。
//! 行政区 `china.sqlite` 仅用于校验/补充省市镇索引,不得反向生成机构。

use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

use crate::cid::{code, parse_cid_number_parts};
use crate::core::{chain_runtime, db::postgres_error_text};
use crate::institution::subjects::{
    EDUCATION_TYPE_CITY_CITIZEN_EDU_COMMITTEE, EDUCATION_TYPE_NATIONAL_CITIZEN_EDU_COMMITTEE,
};
use crate::Db;

const PROJECTION_KEY_PUBLIC_GOV: &str = "public-gov";
const GOV_SOURCE_CHAIN: &str = "CHAIN";

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct GovChainProjectionReport {
    pub(crate) chain_institutions: usize,
    pub(crate) chain_accounts: usize,
    pub(crate) local_institutions: i64,
    pub(crate) local_accounts: i64,
    pub(crate) institution_rows_changed: u64,
    pub(crate) account_rows_changed: u64,
    pub(crate) obsolete_subjects_removed: u64,
    pub(crate) obsolete_accounts_removed: u64,
    pub(crate) chain_genesis_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChainProjectionSnapshot {
    pub(crate) chain_genesis_hash: String,
    pub(crate) chain_block_hash: String,
    pub(crate) chain_block_number: Option<i64>,
    pub(crate) synced_at: String,
    pub(crate) item_count: i64,
    pub(crate) account_count: i64,
}

#[derive(Debug, Clone)]
struct ChainInstitutionProjection {
    cid_number: String,
    cid_full_name: String,
    cid_short_name: String,
    category: &'static str,
    p1: String,
    province_code: String,
    city_code: String,
    town_code: String,
    institution_code: String,
    education_type: Option<String>,
    family_name: Option<String>,
    given_name: Option<String>,
    legal_representative_cid_number: Option<String>,
    legal_representative_account_id: Option<String>,
}

#[derive(Debug, Clone)]
struct ChainAccountProjection {
    cid_number: String,
    province_code: String,
    city_code: Option<String>,
    account_name: String,
    account_id: String,
}

/// 当前链上投影版本。HTTP 列表接口只把它作为缓存游标回传,不再表达本地生成目录版本。
pub(crate) fn current_chain_projection_version(db: &Db) -> Option<String> {
    current_chain_projection_snapshot(db).map(|snapshot| {
        format!(
            "{}:{}:{}:{}:{}",
            snapshot.chain_genesis_hash,
            snapshot.chain_block_hash,
            snapshot
                .chain_block_number
                .map(|v| v.to_string())
                .unwrap_or_default(),
            snapshot.item_count,
            snapshot.account_count
        )
    })
}

/// 当前链上投影锚点。CitizenApp 只把它作为缓存版本/校验线索,不把 OnChina 当真源。
pub(crate) fn current_chain_projection_snapshot(db: &Db) -> Option<ChainProjectionSnapshot> {
    db.with_client(|conn| {
        let row = conn
            .query_opt(
                "SELECT chain_genesis_hash,
                        COALESCE(chain_block_hash, ''),
                        chain_block_number,
                        synced_at::text,
                        item_count,
                        account_count
                 FROM chain_projection_state
                 WHERE projection_key = $1 AND status = 'OK'",
                &[&PROJECTION_KEY_PUBLIC_GOV],
            )
            .map_err(|e| {
                format!(
                    "query chain projection version failed: {}",
                    postgres_error_text(&e)
                )
            })?;
        Ok(row.map(|r| ChainProjectionSnapshot {
            chain_genesis_hash: r.get(0),
            chain_block_hash: r.get(1),
            chain_block_number: r.get(2),
            synced_at: r.get(3),
            item_count: r.get(4),
            account_count: r.get(5),
        }))
    })
    .ok()
    .flatten()
}

pub(crate) fn chain_projection_ready(db: &Db) -> Result<bool, String> {
    db.with_client(|conn| {
        let row = conn
            .query_one(
                "SELECT EXISTS(
                    SELECT 1 FROM chain_projection_state
                    WHERE projection_key = $1 AND status = 'OK'
                 )",
                &[&PROJECTION_KEY_PUBLIC_GOV],
            )
            .map_err(|e| {
                format!(
                    "query chain projection readiness failed: {}",
                    postgres_error_text(&e)
                )
            })?;
        Ok(row.get(0))
    })
}

/// 检查本地公权机构投影是否已经对应当前链 finalized head。
///
/// 本地 PostgreSQL 只是链上 `PublicManage` 的查询缓存。启动时先读取链锚点,
/// 只有 genesis/finalized head/count 都匹配时才跳过全量同步;任一不匹配就重新读链。
pub(crate) fn chain_projection_matches_current_head_blocking(db: &Db) -> Result<bool, String> {
    let chain_genesis_hash = chain_runtime::cached_chain_genesis_hash_hex()
        .ok_or_else(|| "chain genesis hash not initialized".to_string())?;
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("build gov projection anchor runtime failed: {e}"))?;
    let anchor = rt.block_on(chain_runtime::fetch_finalized_anchor())?;
    db.with_client(|conn| {
        let row = conn
            .query_opt(
                "SELECT chain_genesis_hash,
                        COALESCE(chain_block_hash, ''),
                        chain_block_number,
                        item_count,
                        account_count
                 FROM chain_projection_state
                 WHERE projection_key = $1 AND status = 'OK'",
                &[&PROJECTION_KEY_PUBLIC_GOV],
            )
            .map_err(|e| {
                format!(
                    "query chain projection anchor failed: {}",
                    postgres_error_text(&e)
                )
            })?;
        let Some(row) = row else {
            return Ok(false);
        };
        let stored_genesis_hash: String = row.get(0);
        let stored_block_hash: String = row.get(1);
        let stored_block_number: Option<i64> = row.get(2);
        let item_count: i64 = row.get(3);
        let account_count: i64 = row.get(4);
        Ok(stored_genesis_hash == chain_genesis_hash
            && stored_block_hash == anchor.block_hash
            && stored_block_number == Some(anchor.block_number)
            && item_count > 0
            && account_count > 0)
    })
}

/// 从链上唯一真源同步公权机构投影。
pub(crate) fn sync_gov_chain_projection_blocking(
    db: &Db,
) -> Result<GovChainProjectionReport, String> {
    let chain_genesis_hash = chain_runtime::cached_chain_genesis_hash_hex()
        .ok_or_else(|| "chain genesis hash not initialized".to_string())?;
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("build gov projection sync runtime failed: {e}"))?;
    let (anchor, (institutions, accounts)) = rt.block_on(async {
        let anchor = chain_runtime::fetch_finalized_anchor().await?;
        let projection = read_chain_projection().await?;
        Ok::<_, String>((anchor, projection))
    })?;
    write_chain_projection(db, chain_genesis_hash, anchor, institutions, accounts)
}

async fn read_chain_projection(
) -> Result<(Vec<ChainInstitutionProjection>, Vec<ChainAccountProjection>), String> {
    let mut institution_error: Option<String> = None;
    let mut institutions = Vec::new();
    chain_runtime::for_each_chain_institution(|cid, info| {
        if institution_error.is_some() {
            return;
        }
        match parse_chain_institution(cid, info) {
            Ok(item) => institutions.push(item),
            Err(err) => institution_error = Some(err),
        }
    })
    .await?;
    if let Some(err) = institution_error {
        return Err(err);
    }
    // 链上公权机构 CID 必须唯一;重复即投影数据被污染,拒绝写库。
    assert_no_duplicate_chain_cids(&institutions)?;

    let scope_by_cid = institutions
        .iter()
        .map(|item| {
            (
                item.cid_number.clone(),
                (
                    item.province_code.clone(),
                    normalized_city_code(item.city_code.as_str()),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut account_error: Option<String> = None;
    let mut accounts = Vec::new();
    chain_runtime::for_each_chain_institution_account(|item| {
        if account_error.is_some() {
            return;
        }
        match parse_chain_account(item, &scope_by_cid) {
            Ok(account) => accounts.push(account),
            Err(err) => account_error = Some(err),
        }
    })
    .await?;
    if let Some(err) = account_error {
        return Err(err);
    }

    Ok((institutions, accounts))
}

fn parse_chain_institution(
    cid_bytes: Vec<u8>,
    info: chain_runtime::OnChainInstitution,
) -> Result<ChainInstitutionProjection, String> {
    let cid_number = String::from_utf8(cid_bytes)
        .map_err(|_| "chain institution cid_number must be utf-8".to_string())?;
    let parts = parse_cid_number_parts(cid_number.as_str())
        .map_err(|e| format!("chain institution cid_number {cid_number} invalid: {e}"))?;
    if !code::is_public_legal_code(&parts.institution) {
        return Err(format!(
            "chain institution cid_number {cid_number} is not public legal code"
        ));
    }
    let code_from_storage =
        trim_institution_code(info.institution_code.as_slice()).map_err(|e| {
            format!(
                "chain institution {cid_number} {e}, raw={:?}",
                info.institution_code
            )
        })?;
    if code_from_storage != parts.institution_code_text {
        return Err(format!(
            "chain institution {cid_number} code mismatch: cid={} storage={}",
            parts.institution_code_text, code_from_storage
        ));
    }
    let province_code = parts
        .r5
        .get(0..2)
        .ok_or_else(|| format!("chain institution {cid_number} missing province code"))?
        .to_string();
    let city_code = parts
        .r5
        .get(2..5)
        .ok_or_else(|| format!("chain institution {cid_number} missing city code"))?
        .to_string();
    let town_code = String::from_utf8(info.town_code)
        .map_err(|_| format!("chain institution {cid_number} town_code must be utf-8"))?;
    let is_town_institution = matches!(
        code::admin_level(&parts.institution),
        Some(code::AdminLevel::Town)
    );
    if is_town_institution && town_code.trim().is_empty() {
        return Err(format!(
            "chain town institution {cid_number} missing town_code"
        ));
    }
    if !is_town_institution && !town_code.trim().is_empty() {
        return Err(format!(
            "chain non-town institution {cid_number} has town_code"
        ));
    }
    let (family_name, given_name, legal_representative_cid_number, legal_representative_account_id) =
        match info.legal_representative {
            Some(value) => (
                Some(String::from_utf8(value.family_name).map_err(|_| {
                    format!("chain institution {cid_number} family_name must be utf-8")
                })?),
                Some(String::from_utf8(value.given_name).map_err(|_| {
                    format!("chain institution {cid_number} given_name must be utf-8")
                })?),
                Some(String::from_utf8(value.cid_number).map_err(|_| {
                    format!("chain institution {cid_number} legal representative cid must be utf-8")
                })?),
                Some(format!("0x{}", hex::encode(value.account_id))),
            ),
            None => (None, None, None, None),
        };
    Ok(ChainInstitutionProjection {
        cid_full_name: String::from_utf8(info.cid_full_name)
            .map_err(|_| format!("chain institution {cid_number} cid_full_name must be utf-8"))?,
        cid_short_name: String::from_utf8(info.cid_short_name)
            .map_err(|_| format!("chain institution {cid_number} cid_short_name must be utf-8"))?,
        category: "GOV_INSTITUTION",
        p1: if parts.profit { "1" } else { "0" }.to_string(),
        province_code,
        city_code,
        town_code,
        institution_code: parts.institution_code_text,
        education_type: education_type_for_code(&code_from_storage).map(str::to_string),
        family_name,
        given_name,
        legal_representative_cid_number,
        legal_representative_account_id,
        cid_number,
    })
}

fn parse_chain_account(
    item: chain_runtime::OnChainInstitutionAccount,
    scope_by_cid: &BTreeMap<String, (String, Option<String>)>,
) -> Result<ChainAccountProjection, String> {
    let cid_number = String::from_utf8(item.cid_number)
        .map_err(|_| "chain account cid_number must be utf-8".to_string())?;
    let account_name = String::from_utf8(item.account_name)
        .map_err(|_| format!("chain account {cid_number} account_name must be utf-8"))?;
    let Some((province_code, city_code)) = scope_by_cid.get(cid_number.as_str()) else {
        return Err(format!(
            "chain account {cid_number}/{account_name} has no chain institution"
        ));
    };
    Ok(ChainAccountProjection {
        cid_number,
        province_code: province_code.clone(),
        city_code: city_code.clone(),
        account_name,
        account_id: format!("0x{}", hex::encode(item.account_id)),
    })
}

fn write_chain_projection(
    db: &Db,
    chain_genesis_hash: String,
    anchor: chain_runtime::ChainFinalizedAnchor,
    institutions: Vec<ChainInstitutionProjection>,
    accounts: Vec<ChainAccountProjection>,
) -> Result<GovChainProjectionReport, String> {
    if institutions.is_empty() {
        return Err("chain public institution projection is empty".to_string());
    }
    let chain_institutions = institutions.len();
    let chain_accounts = accounts.len();
    db.with_client(move |conn| {
        let mut tx = conn.transaction().map_err(|e| {
            format!(
                "begin gov chain projection sync failed: {}",
                postgres_error_text(&e)
            )
        })?;
        tx.batch_execute(
            "CREATE TEMP TABLE tmp_chain_gov_cids (
                province_code TEXT NOT NULL,
                cid_number TEXT NOT NULL,
                PRIMARY KEY (province_code, cid_number)
             ) ON COMMIT DROP;
             CREATE TEMP TABLE tmp_chain_gov_accounts (
                province_code TEXT NOT NULL,
                cid_number TEXT NOT NULL,
                account_name TEXT NOT NULL,
                PRIMARY KEY (province_code, cid_number, account_name)
             ) ON COMMIT DROP;",
        )
        .map_err(|e| {
            format!(
                "create gov projection temp tables failed: {}",
                postgres_error_text(&e)
            )
        })?;

        let mut institution_rows_changed = 0u64;
        for chunk in institutions.chunks(5_000) {
            insert_tmp_cids(&mut tx, chunk)?;
            institution_rows_changed += upsert_institution_chunk(&mut tx, chunk)?;
        }

        let mut account_rows_changed = 0u64;
        for chunk in accounts.chunks(5_000) {
            insert_tmp_accounts(&mut tx, chunk)?;
            account_rows_changed += upsert_account_chunk(&mut tx, chunk)?;
        }

        let obsolete_accounts_removed = tx
            .execute(
                "DELETE FROM accounts a
                 USING gov g
                 WHERE g.province_code = a.province_code
                   AND g.cid_number = a.cid_number
                   AND g.source = $1
                   AND NOT EXISTS (
                     SELECT 1 FROM tmp_chain_gov_accounts t
                     WHERE t.province_code = a.province_code
                       AND t.cid_number = a.cid_number
                       AND t.account_name = a.account_name
                   )",
                &[&GOV_SOURCE_CHAIN],
            )
            .map_err(|e| {
                format!(
                    "delete obsolete chain accounts failed: {}",
                    postgres_error_text(&e)
                )
            })?;

        let obsolete_subjects_removed = delete_obsolete_chain_institutions(&mut tx)?;
        let local_institutions = count_chain_institutions(&mut tx)?;
        let local_accounts = count_chain_accounts(&mut tx)?;
        tx.execute(
            "INSERT INTO chain_projection_state (
                projection_key, chain_genesis_hash, chain_block_hash, chain_block_number,
                item_count, account_count, status, synced_at
             ) VALUES ($1, $2, $3, $4, $5, $6, 'OK', now())
             ON CONFLICT (projection_key) DO UPDATE SET
                chain_genesis_hash = EXCLUDED.chain_genesis_hash,
                chain_block_hash = EXCLUDED.chain_block_hash,
                chain_block_number = EXCLUDED.chain_block_number,
                item_count = EXCLUDED.item_count,
                account_count = EXCLUDED.account_count,
                status = EXCLUDED.status,
                synced_at = now()",
            &[
                &PROJECTION_KEY_PUBLIC_GOV,
                &chain_genesis_hash,
                &anchor.block_hash,
                &anchor.block_number,
                &i64::try_from(chain_institutions)
                    .map_err(|_| "chain institution count exceeds i64".to_string())?,
                &i64::try_from(chain_accounts)
                    .map_err(|_| "chain account count exceeds i64".to_string())?,
            ],
        )
        .map_err(|e| {
            format!(
                "upsert chain projection state failed: {}",
                postgres_error_text(&e)
            )
        })?;
        tx.commit().map_err(|e| {
            format!(
                "commit gov chain projection sync failed: {}",
                postgres_error_text(&e)
            )
        })?;
        Ok(GovChainProjectionReport {
            chain_institutions,
            chain_accounts,
            local_institutions,
            local_accounts,
            institution_rows_changed,
            account_rows_changed,
            obsolete_subjects_removed,
            obsolete_accounts_removed,
            chain_genesis_hash,
        })
    })
}

fn insert_tmp_cids(
    tx: &mut postgres::Transaction<'_>,
    chunk: &[ChainInstitutionProjection],
) -> Result<(), String> {
    let province_codes = chunk
        .iter()
        .map(|item| item.province_code.clone())
        .collect::<Vec<_>>();
    let cids = chunk
        .iter()
        .map(|item| item.cid_number.clone())
        .collect::<Vec<_>>();
    tx.execute(
        "INSERT INTO tmp_chain_gov_cids(province_code, cid_number)
         SELECT province_code, cid_number
         FROM unnest($1::text[], $2::text[]) AS u(province_code, cid_number)
         ON CONFLICT DO NOTHING",
        &[&province_codes, &cids],
    )
    .map_err(|e| {
        format!(
            "insert tmp chain gov cids failed: {}",
            postgres_error_text(&e)
        )
    })?;
    Ok(())
}

fn insert_tmp_accounts(
    tx: &mut postgres::Transaction<'_>,
    chunk: &[ChainAccountProjection],
) -> Result<(), String> {
    let province_codes = chunk
        .iter()
        .map(|item| item.province_code.clone())
        .collect::<Vec<_>>();
    let cids = chunk
        .iter()
        .map(|item| item.cid_number.clone())
        .collect::<Vec<_>>();
    let account_names = chunk
        .iter()
        .map(|item| item.account_name.clone())
        .collect::<Vec<_>>();
    tx.execute(
        "INSERT INTO tmp_chain_gov_accounts(province_code, cid_number, account_name)
         SELECT province_code, cid_number, account_name
         FROM unnest($1::text[], $2::text[], $3::text[]) AS u(province_code, cid_number, account_name)
         ON CONFLICT DO NOTHING",
        &[&province_codes, &cids, &account_names],
    )
    .map_err(|e| format!("insert tmp chain gov accounts failed: {}", postgres_error_text(&e)))?;
    Ok(())
}

fn upsert_institution_chunk(
    tx: &mut postgres::Transaction<'_>,
    chunk: &[ChainInstitutionProjection],
) -> Result<u64, String> {
    let cids = chunk
        .iter()
        .map(|item| item.cid_number.clone())
        .collect::<Vec<_>>();
    let conflict = tx
        .query_opt(
            "SELECT i.cid_number, i.kind
             FROM ids i
             WHERE i.cid_number = ANY($1)
               AND i.kind <> 'PUBLIC'
             LIMIT 1",
            &[&cids],
        )
        .map_err(|e| {
            format!(
                "query chain gov id conflict failed: {}",
                postgres_error_text(&e)
            )
        })?;
    if let Some(row) = conflict {
        let cid: String = row.get(0);
        let kind: String = row.get(1);
        return Err(format!(
            "chain public institution cid_number {cid} already belongs to local {kind}"
        ));
    }

    let province_codes = chunk
        .iter()
        .map(|item| item.province_code.clone())
        .collect::<Vec<_>>();
    let city_codes = chunk
        .iter()
        .map(|item| normalized_city_code(item.city_code.as_str()))
        .collect::<Vec<Option<String>>>();
    let town_codes = chunk
        .iter()
        .map(|item| (!item.town_code.trim().is_empty()).then(|| item.town_code.clone()))
        .collect::<Vec<Option<String>>>();
    let full_names = chunk
        .iter()
        .map(|item| item.cid_full_name.clone())
        .collect::<Vec<_>>();
    let short_names = chunk
        .iter()
        .map(|item| item.cid_short_name.clone())
        .collect::<Vec<_>>();
    let categories = chunk
        .iter()
        .map(|item| item.category.to_string())
        .collect::<Vec<_>>();
    let p1_values = chunk.iter().map(|item| item.p1.clone()).collect::<Vec<_>>();
    let institution_codes = chunk
        .iter()
        .map(|item| item.institution_code.clone())
        .collect::<Vec<_>>();
    let education_types = chunk
        .iter()
        .map(|item| item.education_type.clone())
        .collect::<Vec<_>>();
    let family_names = chunk
        .iter()
        .map(|item| item.family_name.clone())
        .collect::<Vec<_>>();
    let given_names = chunk
        .iter()
        .map(|item| item.given_name.clone())
        .collect::<Vec<_>>();
    let legal_representative_cid_numbers = chunk
        .iter()
        .map(|item| item.legal_representative_cid_number.clone())
        .collect::<Vec<_>>();
    let legal_representative_accounts = chunk
        .iter()
        .map(|item| item.legal_representative_account_id.clone())
        .collect::<Vec<_>>();
    tx.execute(
        "INSERT INTO ids (cid_number, kind, province_code, city_code)
         SELECT cid_number, 'PUBLIC', province_code, city_code
         FROM unnest($1::text[], $2::text[], $3::text[]) AS u(cid_number, province_code, city_code)
         ON CONFLICT (cid_number) DO UPDATE SET
            province_code = EXCLUDED.province_code,
            city_code = EXCLUDED.city_code
         WHERE ids.kind = 'PUBLIC'
           AND (ids.province_code IS DISTINCT FROM EXCLUDED.province_code
             OR ids.city_code IS DISTINCT FROM EXCLUDED.city_code)",
        &[&cids, &province_codes, &city_codes],
    )
    .map_err(|e| format!("upsert chain gov ids failed: {}", postgres_error_text(&e)))?;

    let changed = tx
        .execute(
            "INSERT INTO subjects (
                cid_number, kind, cid_full_name, cid_short_name,
                category, p1,
                province_code, city_code, town_code, institution_code,
                education_type, private_type, partnership_kind, has_legal_personality,
                parent_cid_number, family_name, given_name,
                legal_representative_cid_number, legal_representative_account_id,
                creator_account_id, updater_account_id, created_at, updated_at,
                institution_source_type
             )
             SELECT
                cid_number, 'PUBLIC', cid_full_name, cid_short_name,
                category, p1,
                province_code, COALESCE(city_code, ''), COALESCE(town_code, ''), institution_code,
                education_type, NULL::text, NULL::text, NULL::boolean,
                NULL::text, family_name, given_name,
                legal_representative_cid_number, legal_representative_account_id,
                NULL::text, NULL::text, now(), now(),
                'CHAIN'
             FROM unnest(
                $1::text[], $2::text[], $3::text[], $4::text[], $5::text[],
                $6::text[], $7::text[], $8::text[], $9::text[], $10::text[],
                $11::text[], $12::text[], $13::text[], $14::text[]
             ) AS u(
                cid_number, cid_full_name, cid_short_name, category, p1,
                institution_code, province_code, city_code, town_code,
                education_type, family_name, given_name,
                legal_representative_cid_number, legal_representative_account_id
             )
             ON CONFLICT (province_code, cid_number) DO UPDATE SET
                kind = EXCLUDED.kind,
                cid_full_name = EXCLUDED.cid_full_name,
                cid_short_name = EXCLUDED.cid_short_name,
                category = EXCLUDED.category,
                p1 = EXCLUDED.p1,
                city_code = EXCLUDED.city_code,
                town_code = EXCLUDED.town_code,
                institution_code = EXCLUDED.institution_code,
                education_type = EXCLUDED.education_type,
                private_type = EXCLUDED.private_type,
                partnership_kind = EXCLUDED.partnership_kind,
                has_legal_personality = EXCLUDED.has_legal_personality,
                parent_cid_number = EXCLUDED.parent_cid_number,
                family_name = EXCLUDED.family_name,
                given_name = EXCLUDED.given_name,
                legal_representative_cid_number = EXCLUDED.legal_representative_cid_number,
                legal_representative_account_id = EXCLUDED.legal_representative_account_id,
                updater_account_id = NULL,
                updated_at = now(),
                institution_source_type = EXCLUDED.institution_source_type
             WHERE subjects.kind IS DISTINCT FROM EXCLUDED.kind
                OR subjects.cid_full_name IS DISTINCT FROM EXCLUDED.cid_full_name
                OR subjects.cid_short_name IS DISTINCT FROM EXCLUDED.cid_short_name
                OR subjects.category IS DISTINCT FROM EXCLUDED.category
                OR subjects.p1 IS DISTINCT FROM EXCLUDED.p1
                OR subjects.city_code IS DISTINCT FROM EXCLUDED.city_code
                OR subjects.town_code IS DISTINCT FROM EXCLUDED.town_code
                OR subjects.institution_code IS DISTINCT FROM EXCLUDED.institution_code
                OR subjects.education_type IS DISTINCT FROM EXCLUDED.education_type
                OR subjects.family_name IS DISTINCT FROM EXCLUDED.family_name
                OR subjects.given_name IS DISTINCT FROM EXCLUDED.given_name
                OR subjects.legal_representative_cid_number IS DISTINCT FROM EXCLUDED.legal_representative_cid_number
                OR subjects.legal_representative_account_id IS DISTINCT FROM EXCLUDED.legal_representative_account_id
                OR subjects.institution_source_type IS DISTINCT FROM EXCLUDED.institution_source_type",
            &[
                &cids,
                &full_names,
                &short_names,
                &categories,
                &p1_values,
                &institution_codes,
                &province_codes,
                &city_codes,
                &town_codes,
                &education_types,
                &family_names,
                &given_names,
                &legal_representative_cid_numbers,
                &legal_representative_accounts,
            ],
        )
        .map_err(|e| format!("upsert chain gov subjects failed: {}", postgres_error_text(&e)))?;

    tx.execute("DELETE FROM private WHERE cid_number = ANY($1)", &[&cids])
        .map_err(|e| {
            format!(
                "delete private rows for chain gov failed: {}",
                postgres_error_text(&e)
            )
        })?;
    tx.execute(
        "INSERT INTO gov (
            cid_number, province_code, city_code, town_code, institution_code,
            source, home_p, home_c
         )
         SELECT cid_number, province_code, city_code, town_code, institution_code,
                $6, NULL::text, NULL::text
         FROM unnest($1::text[], $2::text[], $3::text[], $4::text[], $5::text[])
              AS u(cid_number, province_code, city_code, town_code, institution_code)
         ON CONFLICT (province_code, cid_number) DO UPDATE SET
            city_code = EXCLUDED.city_code,
            town_code = EXCLUDED.town_code,
            institution_code = EXCLUDED.institution_code,
            source = EXCLUDED.source,
            home_p = EXCLUDED.home_p,
            home_c = EXCLUDED.home_c
         WHERE gov.city_code IS DISTINCT FROM EXCLUDED.city_code
            OR gov.town_code IS DISTINCT FROM EXCLUDED.town_code
            OR gov.institution_code IS DISTINCT FROM EXCLUDED.institution_code
            OR gov.source IS DISTINCT FROM EXCLUDED.source
            OR gov.home_p IS DISTINCT FROM EXCLUDED.home_p
            OR gov.home_c IS DISTINCT FROM EXCLUDED.home_c",
        &[
            &cids,
            &province_codes,
            &city_codes,
            &town_codes,
            &institution_codes,
            &GOV_SOURCE_CHAIN,
        ],
    )
    .map_err(|e| format!("upsert chain gov rows failed: {}", postgres_error_text(&e)))?;
    Ok(changed)
}

fn upsert_account_chunk(
    tx: &mut postgres::Transaction<'_>,
    chunk: &[ChainAccountProjection],
) -> Result<u64, String> {
    let cids = chunk
        .iter()
        .map(|item| item.cid_number.clone())
        .collect::<Vec<_>>();
    let province_codes = chunk
        .iter()
        .map(|item| item.province_code.clone())
        .collect::<Vec<_>>();
    let city_codes = chunk
        .iter()
        .map(|item| item.city_code.clone())
        .collect::<Vec<Option<String>>>();
    let account_names = chunk
        .iter()
        .map(|item| item.account_name.clone())
        .collect::<Vec<_>>();
    let accounts = chunk
        .iter()
        .map(|item| Some(item.account_id.clone()))
        .collect::<Vec<Option<String>>>();
    tx.execute(
        "INSERT INTO accounts (
            cid_number, province_code, city_code, account_name, account_id, created_at
         )
         SELECT cid_number, province_code, city_code, account_name, account_id, now()
         FROM unnest($1::text[], $2::text[], $3::text[], $4::text[], $5::text[])
              AS u(cid_number, province_code, city_code, account_name, account_id)
         ON CONFLICT (province_code, cid_number, account_name) DO UPDATE SET
            city_code = EXCLUDED.city_code,
            account_id = EXCLUDED.account_id
         WHERE accounts.city_code IS DISTINCT FROM EXCLUDED.city_code
            OR accounts.account_id IS DISTINCT FROM EXCLUDED.account_id",
        &[
            &cids,
            &province_codes,
            &city_codes,
            &account_names,
            &accounts,
        ],
    )
    .map_err(|e| {
        format!(
            "upsert chain gov accounts failed: {}",
            postgres_error_text(&e)
        )
    })
}

fn delete_obsolete_chain_institutions(tx: &mut postgres::Transaction<'_>) -> Result<u64, String> {
    let rows = tx
        .query(
            "SELECT province_code, cid_number
             FROM gov g
             WHERE g.source = $1
               AND NOT EXISTS (
                 SELECT 1 FROM tmp_chain_gov_cids t
                 WHERE t.province_code = g.province_code
                   AND t.cid_number = g.cid_number
               )",
            &[&GOV_SOURCE_CHAIN],
        )
        .map_err(|e| {
            format!(
                "query obsolete chain gov rows failed: {}",
                postgres_error_text(&e)
            )
        })?;
    let mut total = 0u64;
    for row in rows {
        let province_code: String = row.get(0);
        let cid_number: String = row.get(1);
        tx.execute(
            "DELETE FROM accounts WHERE province_code = $1 AND cid_number = $2",
            &[&province_code, &cid_number],
        )
        .map_err(|e| {
            format!(
                "delete obsolete chain accounts failed: {}",
                postgres_error_text(&e)
            )
        })?;
        tx.execute(
            "DELETE FROM gov WHERE province_code = $1 AND cid_number = $2",
            &[&province_code, &cid_number],
        )
        .map_err(|e| {
            format!(
                "delete obsolete chain gov row failed: {}",
                postgres_error_text(&e)
            )
        })?;
        tx.execute(
            "DELETE FROM subjects WHERE province_code = $1 AND cid_number = $2 AND kind = 'PUBLIC'",
            &[&province_code, &cid_number],
        )
        .map_err(|e| {
            format!(
                "delete obsolete chain subject failed: {}",
                postgres_error_text(&e)
            )
        })?;
        tx.execute(
            "DELETE FROM ids WHERE cid_number = $1 AND kind = 'PUBLIC'",
            &[&cid_number],
        )
        .map_err(|e| {
            format!(
                "delete obsolete chain id failed: {}",
                postgres_error_text(&e)
            )
        })?;
        total = total.saturating_add(1);
    }
    Ok(total)
}

fn count_chain_institutions(tx: &mut postgres::Transaction<'_>) -> Result<i64, String> {
    let row = tx
        .query_one(
            "SELECT COUNT(*)::BIGINT FROM gov WHERE source = $1",
            &[&GOV_SOURCE_CHAIN],
        )
        .map_err(|e| {
            format!(
                "count chain gov institutions failed: {}",
                postgres_error_text(&e)
            )
        })?;
    Ok(row.get(0))
}

fn count_chain_accounts(tx: &mut postgres::Transaction<'_>) -> Result<i64, String> {
    let row = tx
        .query_one(
            "SELECT COUNT(*)::BIGINT
             FROM accounts a
             JOIN gov g ON g.province_code = a.province_code AND g.cid_number = a.cid_number
             WHERE g.source = $1",
            &[&GOV_SOURCE_CHAIN],
        )
        .map_err(|e| {
            format!(
                "count chain gov accounts failed: {}",
                postgres_error_text(&e)
            )
        })?;
    Ok(row.get(0))
}

fn education_type_for_code(institution_code: &str) -> Option<&'static str> {
    match institution_code {
        "NED" => Some(EDUCATION_TYPE_NATIONAL_CITIZEN_EDU_COMMITTEE),
        "CEDU" => Some(EDUCATION_TYPE_CITY_CITIZEN_EDU_COMMITTEE),
        _ => None,
    }
}

fn normalized_city_code(city_code: &str) -> Option<String> {
    let trimmed = city_code.trim();
    (!trimmed.is_empty() && trimmed != "000").then(|| trimmed.to_string())
}

fn trim_institution_code(raw: &[u8]) -> Result<String, String> {
    let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
    let code = std::str::from_utf8(&raw[..end])
        .map_err(|_| "chain institution_code must be ascii".to_string())?
        .trim()
        .to_string();
    if code.is_empty() {
        return Err("chain institution_code is empty".to_string());
    }
    Ok(code)
}

fn assert_no_duplicate_chain_cids(
    institutions: &[ChainInstitutionProjection],
) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for item in institutions {
        if !seen.insert(item.cid_number.as_str()) {
            return Err(format!(
                "duplicate chain public institution {}",
                item.cid_number
            ));
        }
    }
    Ok(())
}

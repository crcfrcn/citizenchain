//! 创世实体落地:把创世时直接上链、未走注册局本地创建流程的实体,播种/回填进对应注册局本地库。
//!
//! 行为契约由本模块测试和创世投影数据结构共同固定。
//! 现阶段两个创世实体:
//! - 创世公民程伟(人主体 CN 号,基金会法定代表人,已有创世 citizen-identity 绑定)→
//!   **按 finalized 链上绑定播种进联邦注册局本地库**,
//!   置于 贵州省(GZ)/绥阳市(与基金会同市)。人主体 CID 去地域化,省市取自基金会机构 CID,
//!   非从程伟号段推。联邦贵州组管理员进绥阳市即可见/可按 CID 搜到;
//!   不加省级特殊视图,不用 PENDING/待补档。
//! - 创世私权机构公民链技术发展基金会(`GZ018-SFGYR`)→ 绥阳市注册局回填(私权,后续 Step 实现)。
//!
//! 铁律:幂等——本地已有则跳过,绝不覆盖任何链下正本,不动现有写入器与推链门。

use chrono::Utc;

use crate::auth::repo::active_node_binding;
use crate::cid::InstitutionCategory;
use crate::core::chain_citizen_identity::{read_finalized_citizen_identity, FinalizedCidStatus};
use crate::core::chain_runtime::{institution_lookup, is_tier1_registry};
use crate::core::db::Db;
use crate::domains::citizens::model::{CitizenRecord, CitizenStatus};
use crate::institution::subjects::model::LegalRepresentative;
use crate::institution::subjects::Institution;

use primitives::cid::china::citizenchain::{
    CITIZENCHAIN_FOUNDATION, LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER,
    LEGAL_REPRESENTATIVE_FAMILY_NAME, LEGAL_REPRESENTATIVE_GIVEN_NAME,
};
use primitives::cid::number::{cid_scope_codes, parse_cid_number_parts};

/// 从机构 CID 唯一解析 (province_code, city_code)。复用 primitives 权威单源
/// `cid_scope_codes`:机构 CID 的 R5 = 省2 + 市3;人主体 CID 去地域化、R5 不载省市,
/// 传入即 fail-closed 返回 Err(杜绝把号段误读成区划码,不自建第二真源)。
fn split_province_city(cid_number: &str) -> Result<(String, String), String> {
    let (province, city) = cid_scope_codes(cid_number.as_bytes())
        .map_err(|e| format!("genesis cid {cid_number} scope invalid: {e}"))?;
    Ok((
        String::from_utf8_lossy(&province).into_owned(),
        String::from_utf8_lossy(&city).into_owned(),
    ))
}

/// 由 CID 号派生一个稳定正整数作为本地 `id`(仅用于列表游标,非业务主键)。
fn stable_citizen_id(cid_number: &str) -> u64 {
    cid_number
        .bytes()
        .fold(0u64, |acc, byte| {
            acc.wrapping_mul(131).wrapping_add(u64::from(byte))
        })
        .max(1)
}

fn citizen_exists(db: &Db, province_code: &str, cid_number: &str) -> Result<bool, String> {
    let province_code = province_code.to_string();
    let cid_number = cid_number.to_string();
    db.with_client(move |conn| {
        let row = conn
            .query_opt(
                "SELECT 1 FROM citizens WHERE province_code = $1 AND cid_number = $2",
                &[&province_code, &cid_number],
            )
            .map_err(|e| format!("query genesis citizen existence failed: {e}"))?;
        Ok(row.is_some())
    })
}

/// 联邦注册局启动时播种创世公民程伟;非联邦节点或已存在则跳过(幂等)。
///
/// 返回 `true` 表示本次真正写入了一条;`false` 表示按规则跳过。
pub(crate) fn seed_genesis_citizen_blocking(db: &Db) -> Result<bool, String> {
    let Some(binding) = active_node_binding(db)? else {
        return Ok(false); // 节点未绑定机构,不播种
    };
    if !is_tier1_registry(binding.institution_code.as_str()) {
        return Ok(false); // 仅联邦注册局(Tier1)持有创世公民
    }

    let cid_number = LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER.to_string();
    // 程伟为人主体 CN 号,去地域化、R5 不载省市;其居住地按创世设定 = 基金会同省市
    // (贵州 GZ / 绥阳 018),故省市均取自基金会机构 CID,让程伟落在联邦库的"贵州/绥阳市"下。
    let (province_code, city_code) = split_province_city(CITIZENCHAIN_FOUNDATION.cid_number)?;

    if citizen_exists(db, province_code.as_str(), cid_number.as_str())? {
        return Ok(false);
    }

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("create genesis citizen chain runtime failed: {e}"))?;
    let snapshot = rt.block_on(read_finalized_citizen_identity(cid_number.as_str()))?;
    if snapshot.cid_status == FinalizedCidStatus::Missing {
        return Err("genesis citizen CID is missing from finalized chain".to_string());
    }
    let binding_revision = snapshot
        .binding_revision
        .ok_or_else(|| "genesis citizen binding revision missing".to_string())?;
    let audit_account_id = snapshot
        .account_id
        .ok_or_else(|| "genesis citizen account binding missing".to_string())?;
    let current_account_id = (snapshot.cid_status == FinalizedCidStatus::Active)
        .then(|| format!("0x{}", hex::encode(audit_account_id)));
    let creator_account_id = format!("0x{}", hex::encode(audit_account_id));
    let finalized_block_hash = format!("0x{}", hex::encode(snapshot.finalized_block_hash));
    let voting_eligible =
        snapshot.cid_status == FinalizedCidStatus::Active && snapshot.voting.is_some();
    let passport_valid_from = snapshot
        .voting
        .as_ref()
        .map(|identity| identity.passport_valid_from.to_string())
        .unwrap_or_default();
    let passport_valid_until = snapshot
        .voting
        .as_ref()
        .map(|identity| identity.passport_valid_until.to_string())
        .unwrap_or_default();
    let now = Utc::now();
    let record = CitizenRecord {
        id: stable_citizen_id(cid_number.as_str()),
        cid_number: cid_number.clone(),
        // 链下正本(护照号/证件/居住地/护照有效期)留空,注册局后续补充,投影/播种不伪造。
        passport_no: String::new(),
        family_name: LEGAL_REPRESENTATIVE_FAMILY_NAME.to_string(),
        given_name: LEGAL_REPRESENTATIVE_GIVEN_NAME.to_string(),
        citizen_sex: String::new(),
        citizen_birth_date: String::new(),
        account_id: current_account_id,
        binding_revision,
        binding_finalized_block_number: Some(i64::from(snapshot.finalized_block_number)),
        binding_finalized_block_hash: Some(finalized_block_hash),
        citizen_status: if snapshot.cid_status == FinalizedCidStatus::Active {
            CitizenStatus::Normal
        } else {
            CitizenStatus::Revoked
        },
        voting_eligible,
        passport_valid_from,
        passport_valid_until,
        status_updated_at: Some(now.timestamp()),
        province_code,
        city_code,
        town_code: String::new(),
        birth_province_code: String::new(),
        birth_city_code: String::new(),
        birth_town_code: String::new(),
        archive_hash: None,
        onchain_tx_hash: None,
        onchain_block_number: None,
        onchain_at: None,
        // 创建来源只是链下审计字段；即使 CID 后续换绑，也不把它解释为当前控制账户。
        creator_account_id,
        created_at: now,
        updater_account_id: None,
        updated_at: now,
    };
    db.upsert_citizen_row(&record)?;
    Ok(true)
}

fn institution_exists(db: &Db, cid_number: &str) -> Result<bool, String> {
    let cid_number = cid_number.to_string();
    db.with_client(move |conn| {
        let row = conn
            .query_opt("SELECT 1 FROM ids WHERE cid_number = $1", &[&cid_number])
            .map_err(|e| format!("query genesis institution existence failed: {e}"))?;
        Ok(row.is_some())
    })
}

/// 市/省注册局启动时回填其作用域内的创世私权机构(基金会)。
///
/// 仅当本节点作用域(其自身机构 CID 的省市)== 基金会所在省市(绥阳)时回填;联邦节点不做
/// (它按 M3 drill-in 投影);本地已存在则跳过(幂等)。需读链(`institution_lookup` 已查
/// PrivateManage);链不可达则返回 Err,由调用方告警跳过,不阻断启动。
///
/// 返回 `true` 表示本次真正写入;`false` 表示按规则跳过。
pub(crate) fn backfill_genesis_private_blocking(db: &Db) -> Result<bool, String> {
    let Some(binding) = active_node_binding(db)? else {
        return Ok(false);
    };
    if is_tier1_registry(binding.institution_code.as_str()) {
        return Ok(false); // 联邦节点走 M3,不做市级私权回填
    }
    let foundation_cid = CITIZENCHAIN_FOUNDATION.cid_number.to_string();
    let (found_province, found_city) = split_province_city(foundation_cid.as_str())?;
    let (node_province, node_city) = split_province_city(binding.institution_cid_number.as_str())?;
    if node_province != found_province || node_city != found_city {
        return Ok(false); // 基金会不在本注册局作用域
    }
    if institution_exists(db, foundation_cid.as_str())? {
        return Ok(false);
    }

    // 读链权威:基金会私权机构信息(institution_lookup 依次查公权/私权,私权命中)。
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("build genesis private backfill runtime failed: {e}"))?;
    let chain_inst = rt
        .block_on(institution_lookup(foundation_cid.as_str()))?
        .ok_or_else(|| format!("genesis foundation {foundation_cid} not found on chain"))?;

    let parts = parse_cid_number_parts(foundation_cid.as_str())
        .map_err(|e| format!("genesis foundation cid invalid: {e}"))?;
    // 中文注释：法定代表人姓名与账户均以链上创世记录为权威，不从本地补造或覆盖。
    let legal_representative = chain_inst
        .legal_representative
        .map(|lr| LegalRepresentative {
            family_name: String::from_utf8_lossy(&lr.family_name).into_owned(),
            given_name: String::from_utf8_lossy(&lr.given_name).into_owned(),
            cid_number: String::from_utf8_lossy(&lr.cid_number).into_owned(),
            account_id: format!("0x{}", hex::encode(lr.account_id)),
        });
    let now = Utc::now();
    // 私权列表 SQL 要求 private_type 非空;链上无此字段,按机构码确定性派生(基金会 SFGY → WELFARE)。
    let derived_private_type =
        crate::domains::private::common::private_type_code_from_institution_code(
            &parts.institution_code_text,
        )
        .map(str::to_string);
    let inst = Institution {
        cid_number: foundation_cid.clone(),
        cid_full_name: Some(String::from_utf8_lossy(&chain_inst.cid_full_name).into_owned()),
        cid_short_name: Some(String::from_utf8_lossy(&chain_inst.cid_short_name).into_owned()),
        category: InstitutionCategory::PrivateInstitution,
        p1: if parts.profit { "1" } else { "0" }.to_string(),
        // 行政区名字不入库(china.sqlite 单源,读时派生),只存代码。
        province_name: String::new(),
        city_name: String::new(),
        town_name: String::new(),
        province_code: found_province,
        city_code: found_city,
        town_code: String::from_utf8_lossy(&chain_inst.town_code).into_owned(),
        institution_code: parts.institution_code_text,
        education_type: None,
        private_type: derived_private_type,
        partnership_kind: None,
        has_legal_personality: Some(true), // 私权法人
        parent_cid_number: None,
        legal_representative,
        legal_representative_photo_path: None,
        legal_representative_photo_name: None,
        legal_representative_photo_mime: None,
        legal_representative_photo_size: None,
        creator_account_id: None, // 链上创世投影无独立创建人
        created_at: now,
    };
    db.upsert_institution_row(&inst)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_province_city_reads_foundation_scope() {
        // 基金会机构 CID → 贵州(GZ)/绥阳(018);创世公民程伟居住地即取此。
        let scope = match split_province_city(CITIZENCHAIN_FOUNDATION.cid_number) {
            Ok(value) => value,
            Err(error) => panic!("foundation scope should decode: {error}"),
        };
        assert_eq!(scope, ("GZ".to_string(), "018".to_string()));
    }

    #[test]
    fn split_province_city_rejects_person_cid() {
        // 程伟为人主体 CN 号,去地域化、R5 不载省市 → fail-closed。
        // 播种须改取基金会省市,绝不对程伟号调用本函数(否则误把号段读成区划)。
        assert!(split_province_city(LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER).is_err());
    }
}

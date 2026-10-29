//! 注册局名册以准确机构 CID 关联行政区；重绑后旧省/同名市行不会混入列表、计数或删除。
use crate::authentication::model::AdminUser;
use crate::authentication::repo::admin_from_row;

pub(crate) fn get_admin_by_id_and_registry_org_conn(
    conn: &mut impl postgres::GenericClient,
    id: u64,
    institution_code: &str,
) -> Result<Option<AdminUser>, String> {
    let id = i64::try_from(id).map_err(|_| "admin id exceeds database capacity")?;
    let row = conn.query_opt("SELECT admin_id,account_id,family_name,given_name,institution_code,built_in,creator_account,created_at,updated_at,city_name,institution_cid FROM admin WHERE admin_id=$1 AND institution_code=$2", &[&id,&institution_code]).map_err(|e| e.to_string())?;
    row.as_ref().map(admin_from_row).transpose()
}

pub(crate) fn city_registry_cid_conn(
    conn: &mut impl postgres::GenericClient,
    province: &str,
    city: &str,
) -> Result<String, String> {
    let province = crate::cid::china::province_code_by_name(province).ok_or("unknown province")?;
    let city = crate::cid::china::area_name_by_codes(province, None, None)
        .and_then(|(name, _, _)| crate::cid::china::city_code_by_name(name, city))
        .ok_or("city outside province")?;
    let rows = conn.query("SELECT s.cid_number FROM subject s JOIN government g ON g.province_code=s.province_code AND g.cid_number=s.cid_number WHERE s.institution_code='CREG' AND s.province_code=$1 AND s.city_code=$2 AND s.state_code='ACTIVE' AND g.source_kind='CHAIN'", &[&province,&city]).map_err(|e| e.to_string())?;
    if rows.len() != 1 {
        return Err("city registry chain projection is missing or ambiguous".into());
    }
    Ok(rows[0].get(0))
}

pub(crate) fn list_city_registry_admins_by_scope_conn(
    conn: &mut impl postgres::GenericClient,
    province: &str,
    city_name: Option<&str>,
    institution_cid: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<(usize, Vec<AdminUser>), String> {
    let province =
        crate::cid::china::province_code_by_name(province).ok_or("admin province scope missing")?;
    let city = city_name
        .map(|name| {
            crate::cid::china::area_name_by_codes(province, None, None)
                .and_then(|(province, _, _)| crate::cid::china::city_code_by_name(province, name))
                .ok_or("city outside province")
        })
        .transpose()?;
    let limit = i64::try_from(limit).map_err(|_| "invalid admin page size")?;
    let offset = i64::try_from(offset).map_err(|_| "invalid admin page offset")?;
    // 缺少历史归属 CID 的行不猜测当前绑定，明确排除并等待获准补齐。
    let filter = " FROM admin a JOIN subject s ON s.cid_number=a.institution_cid WHERE a.institution_code='CREG' AND s.institution_code='CREG' AND s.province_code=$1 AND ($2::text IS NULL OR s.city_code=$2) AND ($3::text IS NULL OR a.institution_cid=$3)";
    let count: i64 = conn
        .query_one(
            format!("SELECT COUNT(*){filter}").as_str(),
            &[&province, &city, &institution_cid],
        )
        .map_err(|e| e.to_string())?
        .get(0);
    let rows = conn.query(format!("SELECT a.admin_id,a.account_id,a.family_name,a.given_name,a.institution_code,a.built_in,a.creator_account,a.created_at,a.updated_at,a.city_name,a.institution_cid{filter} ORDER BY a.admin_id DESC LIMIT $4 OFFSET $5").as_str(), &[&province,&city,&institution_cid,&limit,&offset]).map_err(|e| e.to_string())?;
    Ok((
        usize::try_from(count).map_err(|_| "invalid admin count")?,
        rows.iter()
            .map(admin_from_row)
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

pub(crate) fn count_city_registry_admins_by_city_conn(
    conn: &mut impl postgres::GenericClient,
    province: &str,
    city: &str,
) -> Result<usize, String> {
    let cid = city_registry_cid_conn(conn, province, city)?;
    let count: i64 = conn
        .query_one(
            "SELECT COUNT(*) FROM admin WHERE institution_code='CREG' AND institution_cid=$1",
            &[&cid],
        )
        .map_err(|e| e.to_string())?
        .get(0);
    usize::try_from(count).map_err(|_| "invalid admin count".into())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "由控制台交付本机 onchina_test 数据库后执行隔离 PostgreSQL 集成回归"]
    fn roster_counts_and_delete_guard_use_exact_institution_geography() {
        let sandbox = crate::core::db::IsolatedTestDb::new();
        sandbox.db.with_client(|conn| {
            let mut users=Vec::new();
            for (index,province,city) in [(1i64,"GD","001"),(2,"GZ","001")] {
                let cid=format!("{province}{city}-CREG{}-000000001-2026",primitives::cid::number::checksum_char_m1(&format!("{province}{city}CREG0000000012026"),false));
                conn.execute("INSERT INTO subject(cid_number,record_kind,province_code,city_code,institution_code,state_code,created_at,updated_at) VALUES($1,'PUBLIC',$2,$3,'CREG','ACTIVE',now(),now())", &[&cid,&province,&city]).unwrap();
                conn.execute("INSERT INTO government(cid_number,province_code,city_code,institution_code,source_kind) VALUES($1,$2,$3,'CREG','CHAIN')", &[&cid,&province,&city]).unwrap();
                let user=AdminUser { id:index as u64,account_id:format!("0x{}",format!("{index:02x}").repeat(32)),family_name:"测".into(),given_name:"试".into(),institution_code:"CREG".into(),institution_cid_number:Some(cid),built_in:false,creator_account_id:format!("0x{}","11".repeat(32)),created_at:chrono::Utc::now(),updated_at:None,city_name:"同名市".into() };
                crate::authentication::repo::upsert_admin_conn(conn,&user).unwrap(); users.push(user);
            }
            let (count,rows)=list_city_registry_admins_by_scope_conn(conn,"广东省",None,None,100,0)?;
            assert_eq!(count,1); assert_eq!(rows[0].id,1);
            assert_eq!(count_city_registry_admins_by_city_conn(conn,"广东省","荔湾市")?,1);
            assert!(super::super::city_registry_admins::can_manage_city_registry(Some("广东省"),&users[0]));
            assert!(!super::super::city_registry_admins::can_manage_city_registry(Some("广东省"),&users[1]));
            let mut unknown=users[1].clone(); unknown.id=3; unknown.account_id=format!("0x{}","33".repeat(32)); unknown.institution_cid_number=None;
            crate::authentication::repo::upsert_admin_conn(conn,&unknown).unwrap();
            assert!(!super::super::city_registry_admins::can_manage_city_registry(Some("广东省"),&unknown));
            assert_eq!(list_city_registry_admins_by_scope_conn(conn,"广东省",None,None,100,0)?.0,1);
            Ok(())
        }).unwrap();
    }
}

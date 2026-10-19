use rusqlite::params;

use crate::cid::china;

use super::model::{AddressNameRow, AddressPage, AddressRow};

fn checked_page(page_size: Option<usize>, cursor: Option<usize>) -> (usize, usize) {
    (page_size.unwrap_or(100).clamp(1, 300), cursor.unwrap_or(0))
}

pub(crate) fn list_address_names(
    province_code: &str,
    city_code: &str,
    town_code: &str,
    page_size: Option<usize>,
    cursor: Option<usize>,
) -> Result<AddressPage<AddressNameRow>, String> {
    let (page_size, offset) = checked_page(page_size, cursor);
    china::with_china_connection(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT
                    province_code,
                    city_code,
                    town_code,
                    address_name_code,
                    address_name,
                    COUNT(*) AS address_count
                 FROM addresses
                 WHERE province_code = ?1 AND city_code = ?2 AND town_code = ?3
                 GROUP BY province_code, city_code, town_code, address_name_code, address_name
                 ORDER BY address_name_code
                 LIMIT ?4 OFFSET ?5",
            )
            .map_err(|e| format!("prepare address name query failed: {e}"))?;
        let rows = stmt
            .query_map(
                params![
                    province_code,
                    city_code,
                    town_code,
                    (page_size + 1) as i64,
                    offset as i64
                ],
                |row| {
                    Ok(AddressNameRow {
                        province_code: row.get(0)?,
                        city_code: row.get(1)?,
                        town_code: row.get(2)?,
                        address_name_code: row.get(3)?,
                        address_name: row.get(4)?,
                        address_count: row.get(5)?,
                    })
                },
            )
            .map_err(|e| format!("query address names failed: {e}"))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("read address name row failed: {e}"))?);
        }
        let has_more = items.len() > page_size;
        if has_more {
            items.truncate(page_size);
        }
        Ok(AddressPage {
            items,
            page_size,
            next_cursor: has_more.then_some(offset + page_size),
            has_more,
        })
    })
}

pub(crate) fn list_addresses(
    province_code: &str,
    city_code: &str,
    town_code: &str,
    address_name_code: &str,
    page_size: Option<usize>,
    cursor: Option<usize>,
) -> Result<AddressPage<AddressRow>, String> {
    let (page_size, offset) = checked_page(page_size, cursor);
    china::with_china_connection(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT
                    province_code,
                    city_code,
                    town_code,
                    address_name_code,
                    address_name,
                    address_local_no,
                    address_detail,
                    sort_order
                 FROM addresses
                 WHERE province_code = ?1 AND city_code = ?2 AND town_code = ?3
                   AND address_name_code = ?4
                 ORDER BY address_local_no, address_detail
                 LIMIT ?5 OFFSET ?6",
            )
            .map_err(|e| format!("prepare address query failed: {e}"))?;
        let rows = stmt
            .query_map(
                params![
                    province_code,
                    city_code,
                    town_code,
                    address_name_code,
                    (page_size + 1) as i64,
                    offset as i64
                ],
                |row| {
                    Ok(AddressRow {
                        province_code: row.get(0)?,
                        city_code: row.get(1)?,
                        town_code: row.get(2)?,
                        address_name_code: row.get(3)?,
                        address_name: row.get(4)?,
                        address_local_no: row.get(5)?,
                        address_detail: row.get(6)?,
                        sort_order: row.get(7)?,
                    })
                },
            )
            .map_err(|e| format!("query addresses failed: {e}"))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("read address row failed: {e}"))?);
        }
        let has_more = items.len() > page_size;
        if has_more {
            items.truncate(page_size);
        }
        Ok(AddressPage {
            items,
            page_size,
            next_cursor: has_more.then_some(offset + page_size),
            has_more,
        })
    })
}

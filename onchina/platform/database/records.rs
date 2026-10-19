// 既有结构化数据读写，沿用原事务、字段和查询语义。
impl Db {
    pub(crate) fn cid_full_name_exists(
        &self,
        cid_full_name: &str,
        province_code: Option<&str>,
        city_code: Option<&str>,
        exclude_cid_number: Option<&str>,
    ) -> Result<bool, String> {
        let cid_full_name = cid_full_name.trim().to_string();
        let province_code = province_code.map(str::to_string);
        let city_code = city_code.map(str::to_string);
        let exclude_cid_number = exclude_cid_number.map(str::to_string);
        self.with_client(move |conn| {
            let row = conn
                .query_one(
                    "SELECT EXISTS (
                        SELECT 1 FROM subjects
                        WHERE kind IN ('PUBLIC', 'PRIVATE')
                          AND cid_full_name = $1
                          AND ($2::text IS NULL OR province_code = $2)
                          AND ($3::text IS NULL OR city_code = $3)
                          AND ($4::text IS NULL OR cid_number <> $4)
                     )",
                    &[
                        &cid_full_name,
                        &province_code,
                        &city_code,
                        &exclude_cid_number,
                    ],
                )
                .map_err(|e| format!("query cid_full_name conflict failed: {e}"))?;
            Ok(row.get(0))
        })
    }

    pub(crate) fn get_institution_with_accounts(
        &self,
        cid_number: &str,
    ) -> Result<
        Option<(
            crate::institutions::subjects::Institution,
            Vec<crate::institutions::subjects::InstitutionAccount>,
        )>,
        String,
    > {
        let cid_number = cid_number.trim().to_string();
        self.with_client(move |conn| Self::get_institution_with_accounts_conn(conn, &cid_number))
    }

    pub(crate) fn chain_public_institution_cid_by_code(
        &self,
        institution_code: &str,
    ) -> Result<Option<String>, String> {
        let institution_code = institution_code.trim().to_string();
        self.with_client(move |conn| {
            let rows = conn
                .query(
                    "SELECT cid_number
                     FROM gov
                     WHERE source = 'CHAIN'
                       AND institution_code = $1
                     ORDER BY cid_number ASC
                     LIMIT 2",
                    &[&institution_code],
                )
                .map_err(|e| {
                    format!(
                        "query chain public institution by code failed: {}",
                        crate::core::db::postgres_error_text(&e)
                    )
                })?;
            if rows.len() > 1 {
                return Err(format!(
                    "chain public institution code {institution_code} is not unique in local projection"
                ));
            }
            Ok(rows.first().map(|row| row.get::<_, String>(0)))
        })
    }

    /// `get_institution_with_accounts` 的 conn 级版本,供已持有连接的
    /// 注册局动作派发(admins/actions.rs 注销校验）直接复用,避免嵌套 with_client。
    pub(crate) fn get_institution_with_accounts_conn(
        conn: &mut impl postgres::GenericClient,
        cid_number: &str,
    ) -> Result<
        Option<(
            crate::institutions::subjects::Institution,
            Vec<crate::institutions::subjects::InstitutionAccount>,
        )>,
        String,
    > {
        let cid_number = cid_number.trim().to_string();
        {
            let row = conn
                .query_opt(
                    "SELECT s.cid_number, s.cid_full_name, s.category,
                            s.p1, ''::text AS province_name,
                            ''::text AS city_name, s.province_code, s.city_code, s.institution_code,
                            s.private_type, s.partnership_kind, s.has_legal_personality,
                            s.parent_cid_number, s.creator_account_id, s.created_at,
                            s.cid_full_name, s.cid_short_name,
                            ''::text AS town_name, COALESCE(s.town_code, ''),
                            s.education_type, s.family_name, s.given_name, s.legal_representative_cid_number,
	                            s.legal_representative_photo_path, s.legal_representative_photo_name,
	                            s.legal_representative_photo_mime, s.legal_representative_photo_size,
	                            s.legal_representative_account_id, s.status, s.chain_status
		                     FROM subjects s
		                     LEFT JOIN gov g ON g.province_code = s.province_code AND g.cid_number = s.cid_number
	                     WHERE s.kind IN ('PUBLIC', 'PRIVATE') AND s.cid_number = $1
	                     LIMIT 1",
                    &[&cid_number],
                )
                .map_err(|e| format!("query institution failed: {e}"))?;
            let Some(row) = row else {
                return Ok(None);
            };
            let inst = institution_from_subject_row(&row)?;
            let account_rows = conn
                .query(
                    "SELECT cid_number, account_name, account_id, created_at
                     FROM accounts
                     WHERE cid_number = $1
                     ORDER BY account_name ASC",
                    &[&cid_number],
                )
                .map_err(|e| format!("query institution accounts failed: {e}"))?;
            let mut accounts = Vec::with_capacity(account_rows.len());
            for row in account_rows {
                accounts.push(crate::institutions::subjects::InstitutionAccount {
                    cid_number: row.get(0),
                    account_name: row.get(1),
                    account_id: row.get(2),
                    creator_account_id: None,
                    created_at: row.get(3),
                });
            }
            Ok(Some((inst, accounts)))
        }
    }





    /// 与读合并投影共用 CID 事务锁，避免同 CID 并发创建或链下正本被旧快照覆盖。


    /// next_seq 保存最后已分配值；首次启用只在原子插入时接续既有最大 ID。








    pub(crate) fn upsert_institution_row(
        &self,
        inst: &crate::institutions::subjects::Institution,
    ) -> Result<(), String> {
        let inst = inst.clone();
        self.with_client(move |conn| {
            let mut tx = conn.transaction().map_err(|e| format!("begin institution write failed: {e}"))?;
            crate::authentication::repo::lock_node_binding_conn(&mut tx)?;
            Self::upsert_target_subject_rows(&mut tx, &inst)?;
            tx.commit().map_err(|e| format!("commit institution write failed: {e}"))

        })
    }

    pub(crate) fn legal_representative_account_in_scope(
        &self,
        cid_number: &str,
        scope: &crate::institutions::subjects::service::LegalRepresentativeCitizenScope,
    ) -> Result<Option<String>, String> {
        let cid_number = cid_number.trim().to_string();
        let province_code = scope.province_code().map(str::to_string);
        let city_code = scope.city_code().map(str::to_string);
        self.with_client(move |conn| {
            let row = conn
                .query_opt(
                    "SELECT account_id FROM citizens
                     WHERE cid_number = $1
                       AND citizen_status = 'NORMAL'
                       AND account_id IS NOT NULL
                       AND ($2::text IS NULL OR province_code = $2)
                       AND ($3::text IS NULL OR city_code = $3)
                     LIMIT 1",
                    &[&cid_number, &province_code, &city_code],
                )
                .map_err(|e| format!("query legal representative citizen failed: {e}"))?;
            Ok(row.map(|value| value.get(0)))
        })
    }

    pub(crate) fn search_legal_representative_citizens_in_scope(
        &self,
        q: &str,
        page_size: usize,
        scope: &crate::institutions::subjects::service::LegalRepresentativeCitizenScope,
    ) -> Result<Vec<String>, String> {
        let q = q.trim().to_string();
        let province_code = scope.province_code().map(str::to_string);
        let city_code = scope.city_code().map(str::to_string);
        self.with_client(move |conn| {
            let limit = i64::try_from(page_size).map_err(|_| "page_size too large".to_string())?;
            let rows = conn
                .query(
                    "SELECT cid_number
                     FROM citizens
                     WHERE citizen_status = 'NORMAL'
                       AND ($1::text IS NULL OR province_code = $1)
                       AND ($2::text IS NULL OR city_code = $2)
                       AND cid_number ILIKE '%' || $3 || '%'
                     ORDER BY cid_number ASC
                     LIMIT $4",
                    &[&province_code, &city_code, &q, &limit],
                )
                .map_err(|e| format!("query legal representative citizens failed: {e}"))?;
            Ok(rows
                .iter()
                .map(|row| row.get::<_, String>(0))
                .collect::<Vec<_>>())
        })
    }

    pub(crate) fn upsert_target_subject_rows<C: postgres::GenericClient>(
        conn: &mut C,
        inst: &crate::institutions::subjects::Institution,
    ) -> Result<(), String> {
        let kind = match inst.category {
            crate::cid::InstitutionCategory::PrivateInstitution => "PRIVATE",
            crate::cid::InstitutionCategory::GovInstitution => "PUBLIC",
        };
        let province_code = inst.province_code.clone();
        let city_code = if inst.city_code == "000" || inst.city_code.is_empty() {
            None
        } else {
            Some(inst.city_code.clone())
        };
        let town_code = if inst.town_code.trim().is_empty() {
            None
        } else {
            Some(inst.town_code.clone())
        };
        Self::upsert_target_id_row(
            conn,
            inst.cid_number.as_str(),
            kind,
            province_code.as_str(),
            city_code.as_deref(),
        )?;
        Self::delete_target_rows_outside_scope(
            conn,
            "subjects",
            inst.cid_number.as_str(),
            province_code.as_str(),
        )?;
        let category = institution_category_text(inst.category);
        let legal_representative_photo_size = inst
            .legal_representative_photo_size
            .and_then(|v| i64::try_from(v).ok());
        // 行政区名字不入库(china.sqlite 单源),只写 province_code/city_code/town_code。
        conn.execute(
            "INSERT INTO subjects (
                cid_number, kind, cid_full_name, cid_short_name,
                category, p1,
                province_code, city_code, town_code, institution_code,
                education_type, private_type, partnership_kind, has_legal_personality,
                parent_cid_number, family_name, given_name, legal_representative_cid_number,
                legal_representative_account_id,
                legal_representative_photo_path, legal_representative_photo_name, legal_representative_photo_mime,
                legal_representative_photo_size, creator_account_id, created_at, status, chain_status, updated_at
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18,
                $19, $20, $21, $22, $23, $24, $25, $26, $27, now()
             )
             ON CONFLICT (province_code, cid_number) DO UPDATE SET
                kind = EXCLUDED.kind,
                status = EXCLUDED.status,
                chain_status = EXCLUDED.chain_status,
                cid_full_name = EXCLUDED.cid_full_name,
                cid_short_name = EXCLUDED.cid_short_name,
                category = EXCLUDED.category,
                p1 = EXCLUDED.p1,
                province_code = EXCLUDED.province_code,
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
                legal_representative_photo_path = EXCLUDED.legal_representative_photo_path,
                legal_representative_photo_name = EXCLUDED.legal_representative_photo_name,
                legal_representative_photo_mime = EXCLUDED.legal_representative_photo_mime,
                legal_representative_photo_size = EXCLUDED.legal_representative_photo_size,
                creator_account_id = EXCLUDED.creator_account_id,
                updated_at = now()",
            &[
                &inst.cid_number,
                &kind,
                &inst.cid_full_name,
                &inst.cid_short_name,
                &category,
                &inst.p1,
                &inst.province_code,
                &inst.city_code,
                &inst.town_code,
                &inst.institution_code,
                &inst.education_type,
                &inst.private_type,
                &inst.partnership_kind,
                &inst.has_legal_personality,
                &inst.parent_cid_number,
                &inst.legal_representative.as_ref().map(|value| value.family_name.as_str()),
                &inst.legal_representative.as_ref().map(|value| value.given_name.as_str()),
                &inst.legal_representative.as_ref().map(|value| value.cid_number.as_str()),
                &inst.legal_representative.as_ref().map(|value| value.account_id.as_str()),
                &inst.legal_representative_photo_path,
                &inst.legal_representative_photo_name,
                &inst.legal_representative_photo_mime,
                &legal_representative_photo_size,
                &inst.creator_account_id,
                &inst.created_at,
                &inst.status,
                &inst.chain_status,
            ],
        )
        .map_err(|e| format!("upsert subjects failed: {e}"))?;

        match inst.category {
            crate::cid::InstitutionCategory::PrivateInstitution => {
                Self::delete_target_rows_outside_scope(
                    conn,
                    "private",
                    inst.cid_number.as_str(),
                    province_code.as_str(),
                )?;
                conn.execute("DELETE FROM gov WHERE cid_number = $1", &[&inst.cid_number])
                    .map_err(|e| format!("delete gov row for private subject failed: {e}"))?;
                if let Some(private_type) = &inst.private_type {
                    let has_legal_personality = inst.has_legal_personality.unwrap_or(false);
                    conn.execute(
                        "INSERT INTO private (
                            cid_number, province_code, city_code, code, private_type, partnership_kind,
                            has_legal_personality, p1, parent_cid_number
                         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                         ON CONFLICT (province_code, cid_number) DO UPDATE SET
                            city_code = EXCLUDED.city_code,
                            code = EXCLUDED.code,
                            private_type = EXCLUDED.private_type,
                            partnership_kind = EXCLUDED.partnership_kind,
                            has_legal_personality = EXCLUDED.has_legal_personality,
                            p1 = EXCLUDED.p1,
                            parent_cid_number = EXCLUDED.parent_cid_number",
                        &[
                            &inst.cid_number,
                            &province_code,
                            &city_code,
                            &inst.institution_code,
                            private_type,
                            &inst.partnership_kind,
                            &has_legal_personality,
                            &inst.p1,
                            &inst.parent_cid_number,
                        ],
                    )
                    .map_err(|e| format!("upsert private failed: {e}"))?;
                } else {
                    conn.execute(
                        "DELETE FROM private WHERE cid_number = $1",
                        &[&inst.cid_number],
                    )
                    .map_err(|e| format!("delete non-private typed row failed: {e}"))?;
                }
            }
            crate::cid::InstitutionCategory::GovInstitution => {
                Self::delete_target_rows_outside_scope(
                    conn,
                    "gov",
                    inst.cid_number.as_str(),
                    province_code.as_str(),
                )?;
                conn.execute(
                    "DELETE FROM private WHERE cid_number = $1",
                    &[&inst.cid_number],
                )
                .map_err(|e| format!("delete private row for public subject failed: {e}"))?;
                let home_p: Option<String> = None;
                let home_c: Option<String> = None;
                conn.execute(
                    "INSERT INTO gov (
		                        cid_number, province_code, city_code, town_code, institution_code,
		                        source, home_p, home_c
		                     ) VALUES ($1, $2, $3, $4, $5, 'MANUAL', $6, $7)
		                     ON CONFLICT (province_code, cid_number) DO UPDATE SET
		                        city_code = EXCLUDED.city_code,
		                        town_code = EXCLUDED.town_code,
		                        institution_code = EXCLUDED.institution_code,
		                        home_p = EXCLUDED.home_p,
		                        home_c = EXCLUDED.home_c",
                    &[
                        &inst.cid_number,
                        &province_code,
                        &city_code,
                        &town_code,
                        &inst.institution_code,
                        &home_p,
                        &home_c,
                    ],
                )
                .map_err(|e| format!("upsert gov failed: {e}"))?;
            }
        }
        Ok(())
    }

    /// 机构文档(InstitutionDocument)按其机构 CID 的省/市分区。复用 primitives 权威单源
    /// `cid_scope_codes`(机构 CID R5 = 省2 + 市3;人主体 CID 去地域化 → Err)。
    /// 仅收机构 CID;人主体 CID 或畸形号落兜底 "ZS"/None,不把号段误读成区划码。
    fn scope_codes_from_cid(cid_number: &str) -> (String, Option<String>) {
        let Ok((province, city)) = primitives::cid::number::cid_scope_codes(cid_number.as_bytes())
        else {
            return ("ZS".to_string(), None);
        };
        let province_code = String::from_utf8_lossy(&province).into_owned();
        let city_code = if &city == b"000" {
            None
        } else {
            Some(String::from_utf8_lossy(&city).into_owned())
        };
        (province_code, city_code)
    }

    // 删除所有不合规 CID 号在各号承载表里的行。判定唯一标准 =
    // 过不了 `crate::cid::validate_cid_number_format`。
    // dry_run 时在事务内删完即回滚,只回报计数,不改库。
    pub(crate) fn purge_legacy_cid_rows(&self, dry_run: bool) -> Result<PurgeReport, String> {
        // 号承载表清单,无外键约束,删除顺序无关;主登记表 ids 放最后。
        const CID_TABLES: [&str; 8] = [
            "subjects",
            "citizens",
            "citizen_documents",
            "gov",
            "private",
            "accounts",
            "docs",
            "ids",
        ];
        self.with_client(move |conn| {
            // 1. 收集号全集与 kind(ids 为准,subjects 补孤儿)。
            let mut kind_by_cid: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            for row in conn
                .query("SELECT cid_number, kind FROM ids", &[])
                .map_err(|e| format!("scan ids failed: {e}"))?
            {
                kind_by_cid.entry(row.get(0)).or_insert_with(|| row.get(1));
            }
            for row in conn
                .query("SELECT DISTINCT cid_number, kind FROM subjects", &[])
                .map_err(|e| format!("scan subjects failed: {e}"))?
            {
                kind_by_cid.entry(row.get(0)).or_insert_with(|| row.get(1));
            }

            // 2. 筛旧号:过不了新格式校验的即旧号。
            let legacy: Vec<String> = kind_by_cid
                .keys()
                .filter(|cid| crate::cid::validate_cid_number_format(cid).is_err())
                .cloned()
                .collect();
            let private_count = legacy
                .iter()
                .filter(|cid| kind_by_cid.get(*cid).map(String::as_str) == Some("PRIVATE"))
                .count();
            let citizen_count = legacy
                .iter()
                .filter(|cid| kind_by_cid.get(*cid).map(String::as_str) == Some("CITIZEN"))
                .count();

            if legacy.is_empty() {
                return Ok(PurgeReport {
                    legacy_count: 0,
                    private_count: 0,
                    citizen_count: 0,
                    per_table_deleted: CID_TABLES.iter().map(|table| (*table, 0)).collect(),
                    dry_run,
                });
            }

            // 3. 一事务内逐表删除,记录各表行数。
            let mut tx = conn
                .transaction()
                .map_err(|e| format!("begin purge legacy cid tx failed: {e}"))?;
            let mut per_table_deleted = Vec::with_capacity(CID_TABLES.len());
            for table in CID_TABLES {
                let sql = format!("DELETE FROM {table} WHERE cid_number = ANY($1)");
                let deleted = tx
                    .execute(sql.as_str(), &[&legacy])
                    .map_err(|e| format!("delete legacy cid from {table} failed: {e}"))?;
                per_table_deleted.push((table, deleted));
            }
            if dry_run {
                tx.rollback()
                    .map_err(|e| format!("rollback purge legacy cid dry-run failed: {e}"))?;
            } else {
                tx.commit()
                    .map_err(|e| format!("commit purge legacy cid failed: {e}"))?;
            }

            Ok(PurgeReport {
                legacy_count: legacy.len(),
                private_count,
                citizen_count,
                per_table_deleted,
                dry_run,
            })
        })
    }

    // 扫出"孤儿机构"——subjects 中 town_code 非空、但该
    // (province_code,city_code,town_code) 三元组在行政区划真源 china.sqlite 里不存在
    // (town_code 指向已退役的镇)。判定只走
    // 进程内内存树(crate::cid::china::town_exists),不在 PG 里 join china 数据(PG 无 towns 表)。
    // 白名单:town_code 为空/NULL 的行(市级机构/储委会/部委合法态)永远不是孤儿,直接跳过。
    // 只读扫描,不改库;删除由调用方拿到 cid 列表后逐省级联删。
    pub(crate) fn scan_orphan_institutions(&self) -> Result<Vec<OrphanInstitution>, String> {
        self.with_client(move |conn| {
            let rows = conn
                .query(
                    "SELECT province_code, cid_number, kind,
                            COALESCE(city_code, ''), COALESCE(town_code, ''),
                            ''::text AS town_name, COALESCE(category, ''),
                            COALESCE(institution_code, '')
                     FROM subjects
                     WHERE town_code IS NOT NULL AND town_code <> ''",
                    &[],
                )
                .map_err(|e| {
                    format!(
                        "scan subjects for orphan institutions failed: {}",
                        crate::core::db::postgres_error_text(&e)
                    )
                })?;
            let mut orphans = Vec::new();
            for row in rows {
                let province_code: String = row.get(0);
                let cid_number: String = row.get(1);
                let kind: String = row.get(2);
                let city_code: String = row.get(3);
                let town_code: String = row.get(4);
                let town_name: String = row.get(5);
                let category: String = row.get(6);
                let institution_code: String = row.get(7);
                // 白名单:空 town_code 已在 SQL 过滤;此处再调 town_exists 内存树判定。
                if crate::cid::china::town_exists(&province_code, &city_code, &town_code) {
                    continue;
                }
                orphans.push(OrphanInstitution {
                    province_code,
                    cid_number,
                    kind,
                    city_code,
                    town_code,
                    town_name,
                    category,
                    institution_code,
                });
            }
            Ok(orphans)
        })
    }

    // 把待删孤儿行(subjects/gov/private/accounts)文本导出到备份文件,删除唯一回滚保证。
    // 用 COPY ... TO STDOUT 抓 TSV(不依赖 pg_dump 外部进程),每张表一段,带表名分隔头。
    // 仅命中传入 cid 集合内的行(逐省 province_code + cid ANY 过滤),不会导出无关数据。
    fn export_orphan_backup(
        &self,
        by_province: &std::collections::BTreeMap<String, Vec<String>>,
        backup_path: &str,
    ) -> Result<(), String> {
        use std::io::Write;
        let by_province = by_province.clone();
        let backup_path = backup_path.to_string();
        self.with_client(move |conn| {
            let mut file = std::fs::File::create(&backup_path)
                .map_err(|e| format!("create orphan backup file {backup_path} failed: {e}"))?;
            writeln!(
                file,
                "-- cid purge-orphan-institutions backup\n-- 删除前导出的待删孤儿行(TSV/COPY 格式),删除唯一回滚保证。"
            )
            .map_err(|e| format!("write orphan backup header failed: {e}"))?;
            // 按 (表, 主键关联列) 分别导出;subjects/gov/private/accounts 用 cid_number,
            // docs 用 cid_number,audit 用 target_cid。
            const EXPORTS: [(&str, &str); 6] = [
                ("subjects", "cid_number"),
                ("gov", "cid_number"),
                ("private", "cid_number"),
                ("accounts", "cid_number"),
                ("docs", "cid_number"),
                ("audit", "target_cid"),
            ];
            for (province, cids) in &by_province {
                for (table, key_col) in EXPORTS {
                    writeln!(file, "-- TABLE={table} P_CODE={province}")
                        .map_err(|e| format!("write orphan backup table header failed: {e}"))?;
                    let copy_sql = format!(
                        "COPY (SELECT * FROM {table} WHERE province_code = '{province}' AND {key_col} = ANY(ARRAY[{}])) TO STDOUT",
                        cids
                            .iter()
                            .map(|s| format!("'{}'", s.replace('\'', "''")))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    let mut reader = conn
                        .copy_out(copy_sql.as_str())
                        .map_err(|e| format!("copy out {table} for orphan backup failed: {e}"))?;
                    std::io::copy(&mut reader, &mut file)
                        .map_err(|e| format!("stream {table} copy to orphan backup failed: {e}"))?;
                }
            }
            file.flush()
                .map_err(|e| format!("flush orphan backup file failed: {e}"))?;
            Ok(())
        })
    }

    // 逐省单事务级联删孤儿机构。每省一条事务,WHERE province_code=$1 命中子分区,
    // cid_number = ANY($2) 限定本省孤儿集合;禁止跨省一条 SQL。级联删顺序遵守关联依赖:
    // accounts → docs → audit → gov|private → ids → subjects(子承载表先删,主登记表 ids
    // 与父表 subjects 最后)。gov/private 按 subjects.kind 区分(本方法传入已分好的两类 cid)。
    // audit 关联列为 target_cid。ids 表不分区,仅按 cid_number 删。
    pub(crate) fn delete_orphan_institutions_by_province(
        &self,
        province: &str,
        gov_cids: &[String],
        private_cids: &[String],
    ) -> Result<u64, String> {
        let province = province.to_string();
        let gov_cids = gov_cids.to_vec();
        let private_cids = private_cids.to_vec();
        self.with_client(move |conn| {
            // 本省全部孤儿 cid(gov + private 合集),用于按 province_code 分区命中的表。
            let all_cids: Vec<String> = gov_cids
                .iter()
                .chain(private_cids.iter())
                .cloned()
                .collect();
            if all_cids.is_empty() {
                return Ok(0);
            }
            let mut tx = conn
                .transaction()
                .map_err(|e| format!("begin orphan purge tx for {province} failed: {e}"))?;

            // 1. accounts(province_code 分区,按 cid_number)。
            tx.execute(
                "DELETE FROM accounts WHERE province_code = $1 AND cid_number = ANY($2)",
                &[&province, &all_cids],
            )
            .map_err(|e| format!("delete accounts for {province} failed: {e}"))?;

            // 2. docs(province_code 分区,按 cid_number)。
            tx.execute(
                "DELETE FROM docs WHERE province_code = $1 AND cid_number = ANY($2)",
                &[&province, &all_cids],
            )
            .map_err(|e| format!("delete docs for {province} failed: {e}"))?;

            // 3. audit(province_code 分区,关联列 target_cid)。
            tx.execute(
                "DELETE FROM audit WHERE province_code = $1 AND target_cid = ANY($2)",
                &[&province, &all_cids],
            )
            .map_err(|e| format!("delete audit for {province} failed: {e}"))?;

            // 4. gov / private(province_code 分区,按 kind 区分各自的 cid 集合)。
            if !gov_cids.is_empty() {
                tx.execute(
                    "DELETE FROM gov WHERE province_code = $1 AND cid_number = ANY($2)",
                    &[&province, &gov_cids],
                )
                .map_err(|e| format!("delete gov for {province} failed: {e}"))?;
            }
            if !private_cids.is_empty() {
                tx.execute(
                    "DELETE FROM private WHERE province_code = $1 AND cid_number = ANY($2)",
                    &[&province, &private_cids],
                )
                .map_err(|e| format!("delete private for {province} failed: {e}"))?;
            }

            // 5. ids(不分区,主登记表,仅按 cid_number)。
            tx.execute("DELETE FROM ids WHERE cid_number = ANY($1)", &[&all_cids])
                .map_err(|e| format!("delete ids for {province} failed: {e}"))?;

            // 6. subjects(province_code 分区,父表最后删)。
            let deleted = tx
                .execute(
                    "DELETE FROM subjects WHERE province_code = $1 AND cid_number = ANY($2)",
                    &[&province, &all_cids],
                )
                .map_err(|e| format!("delete subjects for {province} failed: {e}"))?;

            tx.commit()
                .map_err(|e| format!("commit orphan purge tx for {province} failed: {e}"))?;
            Ok(deleted)
        })
    }

    // 参数逐项对应稳定 SQL 过滤维度，封装结构会掩盖调用点的查询范围，故保持显式参数。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn list_institutions_exact(
        &self,
        filter: crate::institutions::subjects::InstitutionListFilter,
        private_type: Option<&str>,
        province_code: &str,
        city_code: Option<&str>,
        keyword: &str,
        cursor: Option<&str>,
        page_size: usize,
    ) -> Result<PageResult<crate::institutions::subjects::InstitutionListRow>, String> {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            if matches!(
                filter,
                crate::institutions::subjects::InstitutionListFilter::Education
            ) {
                return self.list_education_committees_direct(province_code, city_code, page_size);
            }
            // 选定城市后,私权机构空关键字 = 浏览该市全部私权机构(城市级有界)。
            // 省级(无城市)仍返回空,坚持精确搜索以防跨省全量扫描。
            if matches!(
                filter,
                crate::institutions::subjects::InstitutionListFilter::Private
            ) {
                if let Some(city) = city_code {
                    return self.list_private_institutions_by_city_direct(
                        province_code,
                        city,
                        private_type,
                        page_size,
                    );
                }
            }
            return Ok(PageResult {
                items: Vec::new(),
                page_size,
                next_cursor: None,
                has_more: false,
                manifest_version: None,
                catalog_status: None,
            });
        }
        let cursor = decode_db_page_cursor(cursor)?;
        let private_type = private_type
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string);
        let province_code = province_code.to_string();
        let city_code = city_code.map(str::to_string);
        let keyword = keyword.to_string();
        self.with_client(move |conn| {
            let cursor_created_at = cursor.map(|c| c.created_at);
            let fetch_limit = i64::try_from(page_size.saturating_add(1))
                .map_err(|_| "page_size too large".to_string())?;
            // 过滤子句来自 InstitutionListFilter 的静态字面量(教育=手动 JY 行,
            // 非法人按父级属性分流到公权/私权),无注入面;par 是子句依赖的父级别名,
            // 父级允许跨省(私法人全国),故只按 cid_number 关联不限定 province_code。
            let sql = format!(
		                    "SELECT s.cid_number, s.cid_full_name, s.category,
			                                    s.p1, ''::text AS province_name,
			                                    ''::text AS city_name, s.province_code, s.city_code, s.institution_code,
				                                    s.private_type, s.partnership_kind, s.has_legal_personality,
				                                    s.parent_cid_number, s.creator_account_id, s.created_at,
				                                    COALESCE(ac.account_count, 0),
				                                    a.family_name, a.given_name, a.institution_code, s.cid_full_name, s.cid_short_name,
				                                    ''::text AS town_name, COALESCE(s.town_code, ''),
				                                    s.education_type, s.status, s.chain_status
		                             FROM subjects s
		                             LEFT JOIN gov g ON g.province_code = s.province_code AND g.cid_number = s.cid_number
		                             LEFT JOIN subjects par ON par.cid_number = s.parent_cid_number
		                             LEFT JOIN (
	                                SELECT province_code, cid_number, COUNT(*)::BIGINT AS account_count
	                                FROM accounts
	                                WHERE province_code = $1
	                                  AND ($2::text IS NULL OR city_code = $2)
	                                GROUP BY province_code, cid_number
	                             ) ac ON ac.province_code = s.province_code AND ac.cid_number = s.cid_number
	                             LEFT JOIN admins a ON a.account_id = s.creator_account_id
	                             WHERE s.kind IN ('PUBLIC', 'PRIVATE')
	                               {filter_clause}
	                               AND ($6::text IS NULL OR s.private_type = $6)
	                               AND s.province_code = $1
	                               AND ($2::text IS NULL OR s.city_code = $2)
	                               AND (
	                                    s.cid_number = $3
	                                    OR lower(COALESCE(s.cid_full_name, '')) = lower($3)
	                                    OR lower(COALESCE(s.cid_short_name, '')) = lower($3)
	                               )
	                               AND (
	                                    $4::timestamptz IS NULL
	                                    OR s.created_at < $4
	                               )
	                             ORDER BY s.created_at DESC, s.cid_number DESC
	                             LIMIT $5",
                filter_clause = filter.sql_clause(),
            );
            let rows = conn
                .query(
                    sql.as_str(),
                    &[
                        &province_code,
                        &city_code,
                        &keyword,
                        &cursor_created_at,
                        &fetch_limit,
                        &private_type,
                    ],
                )
                .map_err(|e| format!("query subjects failed: {e}"))?;
            let mut output = Vec::with_capacity(rows.len());
            for row in rows {
                // 列布局与 institution_row_from_pg_row 一致,统一走该 helper(含名字派生)。
                let created_at: DateTime<Utc> = row.get(14);
                let cid_number: String = row.get(0);
                let item = institution_row_from_pg_row(&row)?;
                let id = stable_institution_cursor_id(cid_number.as_str());
                output.push((item, created_at, id));
            }
            Ok(page_from_rows(output, page_size))
        })
    }

    fn list_education_committees_direct(
        &self,
        province_code: &str,
        city_code: Option<&str>,
        page_size: usize,
    ) -> Result<PageResult<crate::institutions::subjects::InstitutionListRow>, String> {
        let province_code = province_code.to_string();
        let city_code = city_code.map(str::to_string);
        self.with_client(move |conn| {
            let city_type = crate::institutions::subjects::EDUCATION_TYPE_CITY_CITIZEN_EDU_COMMITTEE;
            let limit = i64::try_from(page_size.saturating_add(1))
                .map_err(|_| "page_size too large".to_string())?;
            // 市详情只直接显示本市确定性市公民教育委员会;
            // 国家公民教育委员会不跨市铺开,学校和 F+JY 分支机构仍走精确搜索。
            let rows = conn
                .query(
                    "SELECT s.cid_number, s.cid_full_name, s.category,
                                    s.p1, ''::text AS province_name,
                                    ''::text AS city_name, s.province_code, s.city_code, s.institution_code,
                                    s.private_type, s.partnership_kind, s.has_legal_personality,
                                    s.parent_cid_number, s.creator_account_id, s.created_at,
                                    COALESCE(ac.account_count, 0),
                                    a.family_name, a.given_name, a.institution_code, s.cid_full_name, s.cid_short_name,
                                    ''::text AS town_name, COALESCE(s.town_code, ''),
                                    s.education_type, s.status, s.chain_status
	                     FROM subjects s
	                     JOIN gov g ON g.province_code = s.province_code AND g.cid_number = s.cid_number
	                     LEFT JOIN (
                        SELECT cid_number, COUNT(*)::BIGINT AS account_count
                        FROM accounts
                        GROUP BY cid_number
                     ) ac ON ac.cid_number = s.cid_number
                     LEFT JOIN admins a ON a.account_id = s.creator_account_id
                     WHERE s.kind = 'PUBLIC'
		                       AND g.source = 'CHAIN'
		                       AND s.institution_code IN ('NED', 'CEDU', 'GUN', 'GSCH')
	                       AND s.education_type = $3
	                       AND s.province_code = $1
	                       AND ($2::text IS NULL OR s.city_code = $2)
	                     ORDER BY
	                        s.province_code ASC,
	                        s.city_code ASC NULLS FIRST,
	                        s.cid_number ASC
	                     LIMIT $4",
                    &[&province_code, &city_code, &city_type, &limit],
                )
                .map_err(|e| format!("query direct education committees failed: {e}"))?;
            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                items.push(institution_row_from_pg_row(&row)?);
            }
            Ok(offset_page_from_window(items, 0, page_size))
        })
    }

    /// 选定城市后浏览该市全部私权机构(联邦 drill-in 已把该市链上私权机构投影进本地库)。
    ///
    /// 城市级作用域有界,不触发跨省全量扫描;省级(无城市)仍坚持精确搜索(见 `list_institutions_exact`)。
    /// `private_type` 存在时按业务类型再筛(如公益组织页只列 WELFARE)。
    fn list_private_institutions_by_city_direct(
        &self,
        province_code: &str,
        city_code: &str,
        private_type: Option<&str>,
        page_size: usize,
    ) -> Result<PageResult<crate::institutions::subjects::InstitutionListRow>, String> {
        let province_code = province_code.to_string();
        let city_code = city_code.to_string();
        let private_type = private_type
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string);
        self.with_client(move |conn| {
            let limit = i64::try_from(page_size.saturating_add(1))
                .map_err(|_| "page_size too large".to_string())?;
            let rows = conn
                .query(
                    "SELECT s.cid_number, s.cid_full_name, s.category,
                                    s.p1, ''::text AS province_name,
                                    ''::text AS city_name, s.province_code, s.city_code, s.institution_code,
                                    s.private_type, s.partnership_kind, s.has_legal_personality,
                                    s.parent_cid_number, s.creator_account_id, s.created_at,
                                    COALESCE(ac.account_count, 0),
                                    a.family_name, a.given_name, a.institution_code, s.cid_full_name, s.cid_short_name,
                                    ''::text AS town_name, COALESCE(s.town_code, ''),
                                    s.education_type, s.status, s.chain_status
	                     FROM subjects s
	                     LEFT JOIN (
                        SELECT cid_number, COUNT(*)::BIGINT AS account_count
                        FROM accounts
                        GROUP BY cid_number
                     ) ac ON ac.cid_number = s.cid_number
                     LEFT JOIN admins a ON a.account_id = s.creator_account_id
                     WHERE s.kind = 'PRIVATE'
	                       AND s.category = 'PRIVATE_INSTITUTION'
	                       AND s.private_type IS NOT NULL
	                       AND ($3::text IS NULL OR s.private_type = $3)
	                       AND s.province_code = $1
	                       AND s.city_code = $2
	                     ORDER BY
	                        s.city_code ASC NULLS FIRST,
	                        s.cid_number ASC
	                     LIMIT $4",
                    &[&province_code, &city_code, &private_type, &limit],
                )
                .map_err(|e| format!("query private institutions by city failed: {e}"))?;
            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                items.push(institution_row_from_pg_row(&row)?);
            }
            Ok(offset_page_from_window(items, 0, page_size))
        })
    }

    // 公权目录查询显式携带省市镇、机构码和分页窗口，避免把权限范围藏入泛化对象。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn list_official_institutions_scope(
        &self,
        province_code: &str,
        city_code: Option<&str>,
        town_code: Option<&str>,
        keyword: &str,
        institution_code_filter: Option<&str>,
        offset: usize,
        page_size: usize,
    ) -> Result<PageResult<crate::institutions::subjects::InstitutionListRow>, String> {
        let keyword = keyword.trim().to_ascii_lowercase();
        let province_code = province_code.to_string();
        let city_code = city_code.map(str::to_string);
        let town_code = town_code.map(str::to_string);
        let institution_code_filter = institution_code_filter.map(str::to_string);
        self.with_client(move |conn| {
            let limit = i64::try_from(page_size.saturating_add(1))
                .map_err(|_| "page_size too large".to_string())?;
            let offset_i64 =
                i64::try_from(offset).map_err(|_| "page offset too large".to_string())?;
	            // 公权目录 = 链上 PublicManage 投影(gov.source=CHAIN)
	            // + 公权下属非法人(F,父级为公法人)。本地 pending/手工行不能作为公权机构真源。
            // 父级只按 cid_number 关联(cid 全局唯一,父级不限定本省分区)。
            let rows = conn
                .query(
                    "SELECT s.cid_number, s.cid_full_name, s.category,
			                                    s.p1, ''::text AS province_name,
			                                    ''::text AS city_name, s.province_code, s.city_code, s.institution_code,
				                                    s.private_type, s.partnership_kind, s.has_legal_personality,
				                                    s.parent_cid_number, s.creator_account_id, s.created_at,
			                                    COALESCE(ac.account_count, 0),
			                                    a.family_name, a.given_name, a.institution_code, s.cid_full_name, s.cid_short_name,
			                                    ''::text AS town_name, COALESCE(s.town_code, ''),
			                                    s.education_type, s.status, s.chain_status
	                             FROM subjects s
	                             LEFT JOIN gov g ON g.province_code = s.province_code AND g.cid_number = s.cid_number
	                             LEFT JOIN subjects par ON par.cid_number = s.parent_cid_number
		                             LEFT JOIN (
	                                SELECT province_code, cid_number, COUNT(*)::BIGINT AS account_count
	                                FROM accounts
	                                WHERE province_code = $1
	                                  AND ($2::text IS NULL OR city_code = $2)
	                                GROUP BY province_code, cid_number
	                             ) ac ON ac.province_code = s.province_code AND ac.cid_number = s.cid_number
	                             LEFT JOIN admins a ON a.account_id = s.creator_account_id
	                             WHERE s.kind IN ('PUBLIC', 'PRIVATE')
			                               AND (
			                                    (s.category = 'GOV_INSTITUTION'
			                                     AND g.cid_number IS NOT NULL
			                                     AND g.source = 'CHAIN'
			                                     AND s.institution_code NOT IN ('NED', 'CEDU', 'GUN', 'SUN', 'JUN', 'GSCH', 'SFSC', 'JSCH'))
		                                    OR (s.institution_code IN ('SFGT', 'SFGP', 'UNIN')
	                                        AND s.institution_code NOT IN ('NED', 'CEDU', 'GUN', 'SUN', 'GSCH', 'SFSC')
	                                        AND par.category = 'GOV_INSTITUTION')
	                               )
	                               AND s.province_code = $1
	                               AND ($2::text IS NULL OR s.city_code = $2)
	                               AND ($7::text IS NULL OR s.town_code = $7 OR COALESCE(s.town_code, '') = '')
	                               AND ($6::text IS NULL OR s.institution_code = $6)
	                               AND (
	                                    $3::text = ''
	                                    OR lower(s.cid_number) LIKE '%' || $3 || '%'
	                                    OR lower(COALESCE(s.cid_full_name, '')) LIKE '%' || $3 || '%'
	                                    OR lower(COALESCE(s.cid_short_name, '')) LIKE '%' || $3 || '%'
	                               )
	                             ORDER BY
		                                s.city_code ASC NULLS LAST,
		                                s.town_code ASC NULLS LAST,
		                                CASE
                                        WHEN s.institution_code IN ('NLG','NSN','NRP','PLG','PSN','PRP','CLEG') THEN 1
                                        WHEN s.institution_code IN ('NJD','PJD','CJUD') THEN 2
                                        WHEN s.institution_code IN ('NSP','PSP','CSUP','FAC','FAU','FIV') THEN 3
                                        WHEN s.institution_code IN ('NRC','PRC','PRB') THEN 4
                                        ELSE 0
                                    END ASC,
	                                COALESCE(s.cid_short_name, '') ASC,
	                                COALESCE(s.cid_full_name, '') ASC,
	                                s.cid_number ASC
	                             LIMIT $4 OFFSET $5",
                    &[&province_code, &city_code, &keyword, &limit, &offset_i64, &institution_code_filter, &town_code],
                )
                .map_err(|e| format!("query official institutions failed: {e}"))?;
            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                items.push(institution_row_from_pg_row(&row)?);
            }
            Ok(offset_page_from_window(items, offset, page_size))
        })
    }
}


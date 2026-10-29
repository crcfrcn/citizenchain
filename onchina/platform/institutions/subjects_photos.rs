//! 法定代表人照片的 PostgreSQL 正本；不使用文件系统保存上传内容。
use super::model::{Institution, LegalRepresentativePhoto};
use crate::Db;

pub(crate) const PHOTO_URL_PREFIX: &str = "/api/institutions/legal-representative/photo/";
pub(crate) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS photo (
        file_path TEXT PRIMARY KEY,
        file_name TEXT NOT NULL,
        mime_type TEXT NOT NULL CHECK (mime_type IN ('image/jpeg', 'image/png', 'image/webp')),
        file_data BYTEA NOT NULL CHECK (octet_length(file_data) BETWEEN 1 AND 5242880),
        uploader_account TEXT NOT NULL,
        institution_cid TEXT UNIQUE,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now()
    );
    CREATE INDEX IF NOT EXISTS legal_representative_photos_pending
        ON photo(created_at) WHERE institution_cid IS NULL;
";

pub(crate) fn valid_photo_path(path: &str) -> bool {
    path.strip_prefix(PHOTO_URL_PREFIX)
        .and_then(|id| {
            uuid::Uuid::parse_str(id)
                .ok()
                .map(|parsed| parsed.to_string() == id)
        })
        .unwrap_or(false)
}

pub(crate) struct StoredPhoto {
    pub mime_type: String,
    pub content: Vec<u8>,
    pub uploader_account_id: String,
    pub institution_cid_number: Option<String>,
}

impl Db {
    pub(crate) fn insert_legal_representative_photo(
        &self,
        file_name: String,
        mime_type: String,
        content: Vec<u8>,
        uploader_account_id: String,
    ) -> Result<LegalRepresentativePhoto, String> {
        let file_path = format!("{PHOTO_URL_PREFIX}{}", uuid::Uuid::new_v4());
        self.with_client(move |conn| {
            let mut tx = conn.transaction().map_err(|e| e.to_string())?;
            tx.execute(
                "DELETE FROM photo
                 WHERE institution_cid IS NULL AND created_at < now() - interval '24 hours'",
                &[],
            ).map_err(|e| format!("clean expired photos failed: {e}"))?;
            tx.execute(
                "INSERT INTO photo
                 (file_path, file_name, mime_type, file_data, uploader_account) VALUES ($1,$2,$3,$4,$5)",
                &[&file_path, &file_name, &mime_type, &content, &uploader_account_id],
            ).map_err(|e| format!("store legal representative photo failed: {e}"))?;
            tx.commit().map_err(|e| format!("commit photo upload failed: {e}"))?;
            Ok(LegalRepresentativePhoto {
                file_path,
                file_name,
                mime_type,
                file_size: content.len() as u64,
            })
        })
    }

    pub(crate) fn read_legal_representative_photo(
        &self,
        path: String,
    ) -> Result<Option<StoredPhoto>, String> {
        self.with_client(move |conn| {
            conn.query_opt(
                "SELECT mime_type, file_data, uploader_account, institution_cid
                 FROM photo WHERE file_path = $1
                   AND (institution_cid IS NOT NULL OR created_at >= now() - interval '24 hours')",
                &[&path],
            )
            .map(|row| {
                row.map(|r| StoredPhoto {
                    mime_type: r.get(0),
                    content: r.get(1),
                    uploader_account_id: r.get(2),
                    institution_cid_number: r.get(3),
                })
            })
            .map_err(|e| format!("read legal representative photo failed: {e}"))
        })
    }

    /// 锁定机构和照片，核验归属；照片只保存在本表，机构查询通过唯一CID关联读取。
    pub(crate) fn save_institution_with_photo(
        &self,
        inst: Institution,
        actor: String,
    ) -> Result<Option<Institution>, String> {
        self.with_client(move |conn| {
            let mut tx = conn.transaction().map_err(|e| e.to_string())?;
            let subject = tx.query_opt(
                "SELECT cid_number FROM subject WHERE province_code = $1 AND cid_number = $2 FOR UPDATE",
                &[&inst.province_code, &inst.cid_number],
            ).map_err(|e| e.to_string())?;
            if subject.is_none() { return Ok(None); }
            let path = inst.photo_path.as_deref().unwrap_or("");
            if !valid_photo_path(path) { return Ok(None); }
            let photo = tx.query_opt(
                "SELECT uploader_account, institution_cid
                 FROM photo WHERE file_path = $1
                   AND (institution_cid IS NOT NULL OR created_at >= now() - interval '24 hours')
                 FOR UPDATE",
                &[&path],
            ).map_err(|e| e.to_string())?;
            let Some(photo) = photo else { return Ok(None); };
            let bound: Option<String> = photo.get(1);
            let owner: String = photo.get(0);
            if match bound.as_deref() {
                Some(cid) => cid != inst.cid_number,
                None => owner != actor,
            } { return Ok(None); }
            Self::upsert_target_subject_rows(&mut tx, &inst)?;
            tx.execute(
                "DELETE FROM photo WHERE institution_cid = $1 AND file_path <> $2",
                &[&inst.cid_number, &path],
            ).map_err(|e| e.to_string())?;
            tx.execute(
                "UPDATE photo SET institution_cid = $1 WHERE file_path = $2",
                &[&inst.cid_number, &path],
            ).map_err(|e| e.to_string())?;
            let stored=Self::get_institution_with_accounts_conn(&mut tx,&inst.cid_number)?.ok_or("institution disappeared during photo update")?.0;
            tx.commit().map_err(|e| format!("commit institution photo failed: {e}"))?;
            Ok(Some(stored))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "由控制台交付本机 onchina_test 隔离数据库后执行"]
    fn institution_photo_uses_one_record_and_checks_ownership() {
        let sandbox = crate::core::db::IsolatedTestDb::new();
        let db = &sandbox.db;
        let actor = format!("0x{}", "11".repeat(32));
        let cid = format!(
            "GZ018-CREG{}-000000001-2026",
            primitives::cid::number::checksum_char_m1("GZ018CREG0000000012026", false)
        );
        let mut inst: Institution = serde_json::from_value(serde_json::json!({
            "status":"ACTIVE", "subjectProperty":"G", "chainStatus":"ACTIVE_ON_CHAIN",
            "cidNumber":cid, "cidFullName":"测试机构", "cidShortName":"测试",
            "category":"GOV_INSTITUTION", "p1":"0", "provinceName":"贵州省",
            "cityName":"绥阳市", "provinceCode":"GZ", "cityCode":"018",
            "institutionCode":"CREG", "createdAt":chrono::Utc::now()
        }))
        .unwrap();
        db.upsert_institution_row(&inst).unwrap();
        let photo = db
            .insert_legal_representative_photo(
                "test.png".into(),
                "image/png".into(),
                b"synthetic-image".to_vec(),
                actor.clone(),
            )
            .unwrap();
        inst.photo_path = Some(photo.file_path.clone());
        inst.photo_name = Some("forged-name".into());
        inst.photo_mime = Some("image/jpeg".into());
        inst.photo_size = Some(999);
        assert!(db
            .save_institution_with_photo(inst.clone(), "other-owner".into())
            .unwrap()
            .is_none());
        let saved = db
            .save_institution_with_photo(inst, actor.clone())
            .unwrap()
            .unwrap();
        assert_eq!(saved.photo_name.as_deref(), Some("test.png"));
        assert_eq!(saved.photo_mime.as_deref(), Some("image/png"));
        assert_eq!(saved.photo_size, Some(15));
        let actual = db.get_institution_with_accounts(&cid).unwrap().unwrap().0;
        assert_eq!(actual.photo_path, saved.photo_path);
        assert_eq!(actual.photo_size, saved.photo_size);
        let replacement = db
            .insert_legal_representative_photo(
                "next.webp".into(),
                "image/webp".into(),
                vec![1, 2],
                actor.clone(),
            )
            .unwrap();
        let mut next = actual;
        next.photo_path = Some(replacement.file_path.clone());
        let saved = db
            .save_institution_with_photo(next, actor)
            .unwrap()
            .unwrap();
        assert_eq!(saved.photo_size, Some(2));
        assert!(db
            .read_legal_representative_photo(photo.file_path)
            .unwrap()
            .is_none());
        let stored = db
            .read_legal_representative_photo(replacement.file_path)
            .unwrap()
            .unwrap();
        assert_eq!(stored.institution_cid_number.as_deref(), Some(cid.as_str()));
        assert_eq!(stored.content, vec![1, 2]);
    }

    #[test]
    fn photo_reference_must_be_a_database_record_url() {
        let id = "e46f0324-1275-42b4-916a-78824e66d320";
        assert!(valid_photo_path(&format!("{PHOTO_URL_PREFIX}{id}")));
        for path in [
            "data/legal-rep-photos/202607/photo.jpg",
            "/etc/passwd",
            "",
            PHOTO_URL_PREFIX,
        ] {
            assert!(!valid_photo_path(path));
        }
        assert!(!valid_photo_path(&format!(
            "{PHOTO_URL_PREFIX}{id}/../photo"
        )));
    }
}

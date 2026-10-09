//! 法定代表人照片的 PostgreSQL 正本；不使用文件系统保存上传内容。
use super::model::{Institution, LegalRepresentativePhoto};
use crate::Db;

pub(crate) const PHOTO_URL_PREFIX: &str = "/api/institutions/legal-representative/photo/";
pub(crate) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS legal_representative_photos (
        file_path TEXT PRIMARY KEY,
        file_name TEXT NOT NULL,
        mime_type TEXT NOT NULL CHECK (mime_type IN ('image/jpeg', 'image/png', 'image/webp')),
        content BYTEA NOT NULL CHECK (octet_length(content) BETWEEN 1 AND 5242880),
        uploader_account_id TEXT NOT NULL,
        institution_cid_number TEXT UNIQUE,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now()
    );
    CREATE INDEX IF NOT EXISTS legal_representative_photos_pending
        ON legal_representative_photos(created_at) WHERE institution_cid_number IS NULL;
    DELETE FROM legal_representative_photos
        WHERE institution_cid_number IS NULL AND created_at < now() - interval '24 hours';
";

pub(crate) fn valid_photo_path(path: &str) -> bool {
    path.strip_prefix(PHOTO_URL_PREFIX)
        .and_then(|id| uuid::Uuid::parse_str(id).ok().map(|parsed| parsed.to_string() == id))
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
                "DELETE FROM legal_representative_photos
                 WHERE institution_cid_number IS NULL AND created_at < now() - interval '24 hours'",
                &[],
            ).map_err(|e| format!("clean expired photos failed: {e}"))?;
            tx.execute(
                "INSERT INTO legal_representative_photos
                 (file_path, file_name, mime_type, content, uploader_account_id) VALUES ($1,$2,$3,$4,$5)",
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
                "SELECT mime_type, content, uploader_account_id, institution_cid_number
                 FROM legal_representative_photos WHERE file_path = $1
                   AND (institution_cid_number IS NOT NULL OR created_at >= now() - interval '24 hours')",
                &[&path],
            ).map(|row| row.map(|r| StoredPhoto {
                mime_type: r.get(0), content: r.get(1),
                uploader_account_id: r.get(2), institution_cid_number: r.get(3),
            })).map_err(|e| format!("read legal representative photo failed: {e}"))
        })
    }

    /// 锁定机构和照片，核验归属，以数据库元数据写回资料；替换和关联在同一事务完成。
    pub(crate) fn save_institution_with_photo(
        &self,
        mut inst: Institution,
        actor: String,
    ) -> Result<Option<Institution>, String> {
        self.with_client(move |conn| {
            let mut tx = conn.transaction().map_err(|e| e.to_string())?;
            let subject = tx.query_opt(
                "SELECT cid_number FROM subjects WHERE province_code = $1 AND cid_number = $2 FOR UPDATE",
                &[&inst.province_code, &inst.cid_number],
            ).map_err(|e| e.to_string())?;
            if subject.is_none() { return Ok(None); }
            let path = inst.legal_representative_photo_path.as_deref().unwrap_or("");
            if !valid_photo_path(path) { return Ok(None); }
            let photo = tx.query_opt(
                "SELECT file_name, mime_type, octet_length(content), uploader_account_id, institution_cid_number
                 FROM legal_representative_photos WHERE file_path = $1
                   AND (institution_cid_number IS NOT NULL OR created_at >= now() - interval '24 hours')
                 FOR UPDATE",
                &[&path],
            ).map_err(|e| e.to_string())?;
            let Some(photo) = photo else { return Ok(None); };
            let bound: Option<String> = photo.get(4);
            let owner: String = photo.get(3);
            if match bound.as_deref() {
                Some(cid) => cid != inst.cid_number,
                None => owner != actor,
            } { return Ok(None); }
            inst.legal_representative_photo_name = Some(photo.get(0));
            inst.legal_representative_photo_mime = Some(photo.get(1));
            inst.legal_representative_photo_size = Some(photo.get::<_, i32>(2) as u64);
            Self::upsert_target_subject_rows(&mut tx, &inst)?;
            tx.execute(
                "DELETE FROM legal_representative_photos WHERE institution_cid_number = $1 AND file_path <> $2",
                &[&inst.cid_number, &path],
            ).map_err(|e| e.to_string())?;
            tx.execute(
                "UPDATE legal_representative_photos SET institution_cid_number = $1 WHERE file_path = $2",
                &[&inst.cid_number, &path],
            ).map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| format!("commit institution photo failed: {e}"))?;
            Ok(Some(inst))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photo_reference_must_be_a_database_record_url() {
        let id = "e46f0324-1275-42b4-916a-78824e66d320";
        assert!(valid_photo_path(&format!("{PHOTO_URL_PREFIX}{id}")));
        for path in ["data/legal-rep-photos/202607/photo.jpg", "/etc/passwd", "", PHOTO_URL_PREFIX] {
            assert!(!valid_photo_path(path));
        }
        assert!(!valid_photo_path(&format!("{PHOTO_URL_PREFIX}{id}/../photo")));
    }
}

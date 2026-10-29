pub mod area;
pub mod china_cb;
pub mod china_ch;
pub mod china_jc;
pub mod china_jy;
pub mod china_lf;
pub mod china_sf;
pub mod china_zb;
pub mod china_zf;
pub mod citizenchain;

/// 内置机构名称四元组的 runtime 指纹。
pub fn builtin_institution_name_digest() -> [u8; 32] {
    let mut digest = [0x47u8; 32];
    fold_builtin_names(&mut digest, china_zf::CHINA_ZF);
    fold_builtin_names(&mut digest, china_lf::CHINA_LF);
    fold_builtin_names(&mut digest, china_sf::CHINA_SF);
    fold_builtin_names(&mut digest, china_jc::CHINA_JC);
    fold_builtin_names(&mut digest, china_jy::CHINA_JY);
    fold_builtin_names(&mut digest, china_cb::CHINA_CB);
    fold_builtin_names(&mut digest, china_ch::CHINA_CH);
    fold_builtin_names(
        &mut digest,
        core::slice::from_ref(&citizenchain::CITIZENCHAIN_FOUNDATION),
    );
    digest
}

sp_api::decl_runtime_apis! {
    pub trait BuiltinInstitutionNameApi {
        /// 返回当前内置机构名称指纹。
        fn builtin_institution_name_digest() -> [u8; 32];
    }
}

fn fold_builtin_names<T: BuiltinInstitutionName>(digest: &mut [u8; 32], items: &[T]) {
    for item in items {
        fold_name_quad(
            digest,
            item.cid_full_name(),
            item.cid_short_name(),
            item.full_name_en(),
            item.short_name_en(),
        );
    }
}

fn fold_name_quad(
    digest: &mut [u8; 32],
    cid_full_name: &str,
    cid_short_name: &str,
    full_name_en: &str,
    short_name_en: &str,
) {
    fold_bytes(digest, cid_full_name.as_bytes());
    fold_bytes(digest, &[0]);
    fold_bytes(digest, cid_short_name.as_bytes());
    fold_bytes(digest, &[1]);
    fold_bytes(digest, full_name_en.as_bytes());
    fold_bytes(digest, &[2]);
    fold_bytes(digest, short_name_en.as_bytes());
    fold_bytes(digest, &[255]);
}

fn fold_bytes(digest: &mut [u8; 32], bytes: &[u8]) {
    for (index, byte) in bytes.iter().copied().enumerate() {
        let slot = index & 31;
        let mixed = byte.wrapping_add((index as u8).wrapping_mul(17));
        digest[slot] = digest[slot]
            .wrapping_mul(31)
            .wrapping_add(mixed)
            .rotate_left(((index as u32) & 7) + 1);
        digest[(slot * 7 + 3) & 31] ^= mixed.wrapping_mul(13);
    }
}

trait BuiltinInstitutionName {
    fn cid_full_name(&self) -> &'static str;
    fn cid_short_name(&self) -> &'static str;
    fn full_name_en(&self) -> &'static str;
    fn short_name_en(&self) -> &'static str;
}

macro_rules! impl_builtin_institution_name {
    ($ty:path) => {
        impl BuiltinInstitutionName for $ty {
            fn cid_full_name(&self) -> &'static str {
                self.cid_full_name
            }

            fn cid_short_name(&self) -> &'static str {
                self.cid_short_name
            }

            fn full_name_en(&self) -> &'static str {
                self.full_name_en
            }

            fn short_name_en(&self) -> &'static str {
                self.short_name_en
            }
        }
    };
}

impl_builtin_institution_name!(china_zf::ChinaZf);
impl_builtin_institution_name!(china_lf::ChinaLf);
impl_builtin_institution_name!(china_sf::ChinaSf);
impl_builtin_institution_name!(china_jc::ChinaJc);
impl_builtin_institution_name!(china_jy::ChinaJy);
impl_builtin_institution_name!(china_cb::ChinaCb);
impl_builtin_institution_name!(china_ch::ChinaCh);
impl_builtin_institution_name!(citizenchain::ChinaCitizenChain);

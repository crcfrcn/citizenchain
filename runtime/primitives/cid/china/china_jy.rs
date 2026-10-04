//! 公民教育委员会机构常量。

use hex_literal::hex;

/// 公民教育委员会机构常量。
pub struct ChinaJy {
    pub cid_full_name: &'static str,
    pub cid_short_name: &'static str,
    pub cid_full_name_en: &'static str,
    pub cid_short_name_en: &'static str,
    pub cid_number: &'static str,
    pub main_account: [u8; 32],
    pub fee_account: [u8; 32],
}

pub const CHINA_JY: &[ChinaJy] = &[ChinaJy {
    cid_full_name: "中华民族联邦共和国公民教育委员会",
    cid_short_name: "国家教委会",
    cid_full_name_en: "Citizen Education Committee of the Federal Republic of the Chinese Nation",
    cid_short_name_en: "National Education Committee",
    cid_number: "BP001-NED0H-689724263-2026",
    main_account: hex!("506ac991dcbcaa7e09bece1696b425e0c4e73e0e0610b270a4343f8a99dd1bc1"),
    fee_account: hex!("3c4fb289ce5e146b9a33bf1fe74d5e9d2b28e782460aaaa870e6f510c5af1a5d"),
}];

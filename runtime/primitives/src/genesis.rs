//! 创世常量。

/// 创世宣言。
pub const CITIZENS: &str = r#"先有人类后有国家，是公民建立国家，国家是公民的国家，是公民治理国家，而不是国家统治公民，公民没有爱国的义务；国家政权的建立其基本原则是保护公民的生命权、自由权、财产权、反抗压迫权和选举与被选举权不受任何的非法侵犯，当国家政权无法保证这一基本原则时，公民有权有义务推翻这个政权，建立一个以保障公民生命权、自由权、财产权、反抗压迫权和选举与被选举权为基本原则的政权。————《公民宪法》程伟"#;
pub const COUNTRY: &str = r#"中华民族联邦共和国国家名称是基于中华各民族悠久历史与璀璨文化的沉淀，全称为：中华民族联邦共和国，简称为：中华联邦；中华民族联邦共和国是致力于推行“公民主义”的———「公民治理国家（民治）、实行民主共和（民主）、保障公民权利（民权）、建设民生社会（民生）、复兴民族文化（民族）」———联邦制共和国。————《公民宪法》程伟"#;

/// 创世人口。
pub const GENESIS_CITIZEN_MAX: u64 = 1_443_497_378; // 中共第7次人口普查的总人口数，作为创世人口数量

/// 创世发行,单位:分。
pub const GENESIS_ISSUANCE: u128 = 14_434_973_780_000; // 每人100元的创世发行总量，单位为分

/// 两和基金创世发行,单位:分。
pub const HE_FUND_ISSUANCE: u128 = 19_581_850_196_600; // 195,818,501,966.00 元

/// 创世法律版本标签。
pub struct GenesisLawVersionLabel {
    pub law_id: u64,
    pub version: u32,
    pub title: &'static str,
    pub title_en: &'static str,
}
/// 公民宪法创世版本标签。
pub const GENESIS_LAW_VERSION_LABELS: &[GenesisLawVersionLabel] = &[GenesisLawVersionLabel {
    law_id: 0,
    version: 1,
    title: "创世版",
    title_en: "Genesis Edition",
}];

use sp_std::vec::Vec;

// 立法院 Runtime API:供客户端浏览链上法律。
sp_api::decl_runtime_apis! {
    pub trait LegislationApi {
        /// 列出指定层级和行政区的法律 ID。
        fn list_laws(tier: u8, scope_code: u32) -> Vec<u64>;

        /// 读取 SCALE 编码的法律主体。
        fn law(law_id: u64) -> Option<Vec<u8>>;

        /// 读取 SCALE 编码的法律版本。
        fn law_version(law_id: u64, version: u32) -> Option<Vec<u8>>;

        /// 读取 SCALE 编码的法律版本标签。
        fn law_version_label(law_id: u64, version: u32) -> Option<Vec<u8>>;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genesis_issuance_matches_population() {
        // 创世发行 = 人口 × 10_000 分。
        assert_eq!(GENESIS_ISSUANCE, GENESIS_CITIZEN_MAX as u128 * 10_000u128);
    }

    #[test]
    fn he_fund_issuance_matches_whitepaper() {
        // 两和基金发行 = 195,818,501,966.00 元 × 100 分/元。
        assert_eq!(HE_FUND_ISSUANCE, 195_818_501_966u128 * 100);
    }

    #[test]
    fn genesis_law_version_label_is_constitution_genesis() {
        let label = &GENESIS_LAW_VERSION_LABELS[0];
        assert_eq!(label.law_id, 0);
        assert_eq!(label.version, 1);
        assert_eq!(label.title, "创世版");
        assert_eq!(label.title_en, "Genesis Edition");
    }
}

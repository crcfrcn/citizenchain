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

/// 法律地域：省码保留原两位大写字母，市号沿用原省内三位十进制编号。
/// SCALE 固定为两字节省码 + 两字节小端市号；全国四零字节保持创世宪法原布局。
/// 全国值只用于宪法/国家层级，不是解析失败时的默认值，也不是国家代码 CN。
#[derive(
    codec::Encode, codec::Decode, codec::DecodeWithMemTracking, Clone, Copy,
    PartialEq, Eq, Debug, scale_info::TypeInfo, codec::MaxEncodedLen,
)]
pub struct LawRegion {
    pub province_code: [u8; 2],
    pub city_number: u16,
}

impl LawRegion {
    pub const NATIONAL: Self = Self { province_code: [0; 2], city_number: 0 };

    /// 只消费现有省市解析结果；不重新定义 CID 或区划编码。
    pub fn from_codes(tier: u8, province: &[u8], city: &[u8]) -> Result<Self, &'static str> {
        match tier {
            0 | 1 => Ok(Self::NATIONAL),
            2 | 3 => {
                let province_code: [u8; 2] = province.try_into().map_err(|_| "invalid law province")?;
                if !province_code.iter().all(u8::is_ascii_uppercase) {
                    return Err("invalid law province");
                }
                let city_number = if tier == 3 {
                    if city.len() != 3 || !city.iter().all(u8::is_ascii_digit) {
                        return Err("invalid law city");
                    }
                    city.iter().fold(0u16, |value, byte| value * 10 + u16::from(*byte - b'0'))
                } else { 0 };
                Ok(Self { province_code, city_number })
            }
            _ => Err("invalid law tier"),
        }
    }

    /// 唯一机构地域来源仍为原 cid_scope_codes；国家机构的所在地不改变全国法律范围。
    pub fn from_cid(tier: u8, cid: &[u8]) -> Result<Self, &'static str> {
        let (province, city) = crate::cid::number::cid_scope_codes(cid)?;
        Self::from_codes(tier, &province, &city)
    }

    /// 解码后的形状检查；写入时还必须与已授权机构 CID 派生值相等。
    pub fn matches_tier(self, tier: u8) -> bool {
        match tier {
            0 | 1 => self == Self::NATIONAL,
            2 => self.province_code.iter().all(u8::is_ascii_uppercase) && self.city_number == 0,
            3 => self.province_code.iter().all(u8::is_ascii_uppercase) && self.city_number <= 999,
            _ => false,
        }
    }
}

// 立法院 Runtime API:供客户端浏览链上法律。
sp_api::decl_runtime_apis! {
    #[api_version(2)]
    pub trait LegislationApi {
        /// 列出指定层级和行政区的法律 ID。
        fn list_laws(tier: u8, region: LawRegion) -> Vec<u64>;

        /// 读取 SCALE 编码的法律主体。
        fn law(law_id: u64) -> Option<Vec<u8>>;

        /// 读取 SCALE 编码的法律版本。
        fn law_version(law_id: u64, version: u32) -> Option<Vec<u8>>;

        /// 读取 SCALE 编码的法律版本标签。
        fn law_version_label(law_id: u64, version: u32) -> Option<Vec<u8>>;
    }
}

/// 唯一合法的创世区块哈希；节点守卫直接使用编译常量，不接受配置覆盖。
pub const GENESIS_HASH: [u8; 32] =
    hex_literal::hex!("18847a5dfd263272f2e7727836fe6582f8c4463ff48609df7b96d5e4d9dd24dd");

/// 唯一合法的创世状态根；运行期区块的状态根不与此值比较。
pub const GENESIS_STATE_ROOT: [u8; 32] =
    hex_literal::hex!("1f74a2ca094fc3ebb2143f504d807a6b4f4f9b0a3d13ac808ae84efc7cb12111");

// 链身份 Runtime API：验证并读取当前链状态中保存的实际创世哈希。
sp_api::decl_runtime_apis! {
    #[api_version(1)]
    pub trait ChainIdentityApi {
        /// 返回已与冻结身份核对的实际创世哈希；缺失、零值或不一致时调用失败。
        fn genesis_hash() -> [u8; 32];
    }
}

#[cfg(test)]
// 测试仅使用固定公开夹具；断言解包不进入生产派生路径。
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn law_region_keeps_letters_city_numbers_and_frozen_national_bytes() {
        use codec::{Decode, Encode};
        assert_eq!(LawRegion::NATIONAL.encode(), 0u32.encode());
        assert_eq!(LawRegion::from_codes(2, b"GD", b"001").unwrap().encode(), [b'G', b'D', 0, 0]);
        let city = LawRegion::from_codes(3, b"GD", b"001").unwrap();
        assert_eq!(city.encode(), [b'G', b'D', 1, 0]);
        assert_ne!(city, LawRegion::from_codes(3, b"GZ", b"001").unwrap());
        assert_eq!(LawRegion::decode(&mut &city.encode()[..]).unwrap(), city);
        assert_eq!(LawRegion::from_codes(3, b"GD", b"999").unwrap().city_number, 999);
        // 不新增“市号必须大于零”的编号规则；合法性仍由原 CID/字典入口负责。
        assert_eq!(LawRegion::from_codes(3, b"GD", b"000").unwrap().city_number, 0);
        for (tier, province, city) in [(4, &b"GD"[..], &b"001"[..]), (2, b"gd", b""),
            (2, b"G", b""), (3, b"GD", b"1"), (3, b"GD", b"0a1")] {
            assert!(LawRegion::from_codes(tier, province, city).is_err());
        }
        assert!(!LawRegion::NATIONAL.matches_tier(2));
        assert!(!(LawRegion { province_code: *b"GD", city_number: 1000 }).matches_tier(3));
        assert!(LawRegion::from_cid(2, b"invalid").is_err());
        assert_eq!(crate::cid::code::COUNTRY_CN, *b"CN");
    }

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

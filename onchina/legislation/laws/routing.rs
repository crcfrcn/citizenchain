//! 法律案宪法路由:层级 × 是否教育 → houses / executive / legislature 机构码序列(单源)。
//!
//! 本表只定**机构码**层级路由(宪法第45/46/100/104–108条);`service` 再按机构码
//! 与行政区解析唯一 CID。机构码取自 `primitives::cid::code` 真源:
//! - 国家众议会 NRP / 国家参议会 NSN / 国家教委会 NED / 国家立法院 NLG / 总统府 PRS;
//! - 省众议会 PRP / 省参议会 PSN / 省立法院 PLG / 省政府 PGV;
//! - 市立法会 CLEG / 市政府 CGOV。
//!
//! 解耦:`actor_cid_number`(实际发起机构)与 `houses`(表决院序列)独立——市级教委会/自治会
//! 发起时 actor 不等于 `houses[0]`(后者恒为市立法会),由 `service` 分别解析 CID。
//!

/// 一条法律案路由(机构码层级；CID 在运行时解析)。
pub struct LawRouting {
    /// 表决院序列(发起院在前、终审院在后;市级单院)。
    pub houses: Vec<[u8; 4]>,
    /// 行政签署机构(总统府 / 省政府 / 市政府)。
    pub executive: [u8; 4],
    /// 三人会签归口院(国家/省立法院;市级无会签 = None)。
    pub legislature: Option<[u8; 4]>,
}

/// 表决类型是否教育类(1 常规教育 / 3 重要教育,对齐链 VoteType::is_education)。
pub fn vote_type_is_education(vote_type: u8) -> bool {
    vote_type == 1 || vote_type == 3
}

/// 层级(0 宪法 / 1 国家 / 2 省 / 3 市)+ 是否教育 → 路由。
///
/// 宪法案(0)按国家立法院处理(修宪走国家众议会→参议会,非教育);
/// 省级无教委会,故 (2, true) 返回 None(省教育案不存在);非法组合返回 None。
pub fn routing_for(tier: u8, is_education: bool) -> Option<LawRouting> {
    match (tier, is_education) {
        // 国家·非教育 与 宪法(修宪):众议会发起 → 参议会终审;总统签署;国家立法院会签。
        (1, false) | (0, false) => Some(LawRouting {
            houses: vec![*b"NRP\0", *b"NSN\0"],
            executive: *b"PRS\0",
            legislature: Some(*b"NLG\0"),
        }),
        // 国家·教育:教委会本会先表决 → 参议会终审。
        (1, true) => Some(LawRouting {
            houses: vec![*b"NED\0", *b"NSN\0"],
            executive: *b"PRS\0",
            legislature: Some(*b"NLG\0"),
        }),
        // 省·非教育:省众议会发起 → 省参议会终审;省长签署;省立法院会签。
        (2, false) => Some(LawRouting {
            houses: vec![*b"PRP\0", *b"PSN\0"],
            executive: *b"PGV\0",
            legislature: Some(*b"PLG\0"),
        }),
        // 市(非教育/教育):单院市立法会表决;市长签署;无三人会签。
        (3, _) => Some(LawRouting {
            houses: vec![*b"CLEG"],
            executive: *b"CGOV",
            legislature: None,
        }),
        // 省教育案不存在(省无教委会);宪法教育、其它组合非法。
        _ => None,
    }
}

#[cfg(test)]
// 路由夹具必须映射到确定结果，断言式解包用于暴露配置回归。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn national_routing_two_houses_with_legislature() {
        let non_edu = routing_for(1, false).expect("national non-edu");
        assert_eq!(non_edu.houses, vec![*b"NRP\0", *b"NSN\0"]);
        assert_eq!(non_edu.executive, *b"PRS\0");
        assert_eq!(non_edu.legislature, Some(*b"NLG\0"));

        // 国家教育案:发起院换教委会,终审仍参议会。
        let edu = routing_for(1, true).expect("national edu");
        assert_eq!(edu.houses, vec![*b"NED\0", *b"NSN\0"]);
    }

    #[test]
    fn constitution_routes_like_national_non_education() {
        let constitution = routing_for(0, false).expect("constitution amend");
        assert_eq!(constitution.houses, vec![*b"NRP\0", *b"NSN\0"]);
        assert_eq!(constitution.legislature, Some(*b"NLG\0"));
    }

    #[test]
    fn provincial_two_houses_municipal_single_house() {
        let province = routing_for(2, false).expect("provincial");
        assert_eq!(province.houses, vec![*b"PRP\0", *b"PSN\0"]);
        assert_eq!(province.executive, *b"PGV\0");

        // 市级单院,无会签。
        for is_edu in [false, true] {
            let city = routing_for(3, is_edu).expect("municipal");
            assert_eq!(city.houses, vec![*b"CLEG"]);
            assert_eq!(city.executive, *b"CGOV");
            assert_eq!(city.legislature, None);
        }
    }

    #[test]
    fn provincial_education_and_illegal_combos_have_no_routing() {
        // 省无教委会 → 省教育案不存在。
        assert!(routing_for(2, true).is_none());
        // 宪法教育案非法。
        assert!(routing_for(0, true).is_none());
        // 未知层级。
        assert!(routing_for(9, false).is_none());
    }

    #[test]
    fn vote_type_education_flags_match_chain() {
        assert!(!vote_type_is_education(0)); // 常规
        assert!(vote_type_is_education(1)); // 常规教育
        assert!(!vote_type_is_education(2)); // 重要
        assert!(vote_type_is_education(3)); // 重要教育
        assert!(!vote_type_is_education(4)); // 特别
    }
}

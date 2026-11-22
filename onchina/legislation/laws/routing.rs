//! 按法律层级和教育表决类型确定两院及执行机构路由。

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
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/laws_routing.rs"]
mod tests;

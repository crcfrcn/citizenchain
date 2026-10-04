//! 机构作用域规则
//!
//! 按登录管理员的机构行政层级(admin_level)派生其可见/可操作的数据范围
//! (VisibleScope)。所有 list/CRUD API 都应先派生 scope,再用它过滤数据。
//!
//! 五档范围:
//! - 全国(NATIONAL,部委等)        → 不限省/市/镇
//! - 省级(PROVINCE / 联邦注册局)   → 本省,所有市
//! - 市级(CITY)                    → 本市,所有镇
//! - 镇级(TOWN)                    → 本镇
//! - 自机构(私权法人/非法人,无层级)→ 暂沿用本市范围(精确"仅本机构"范围见后续 admin 泛化卡)

use crate::auth::login::AdminAuthContext;

/// 登录管理员可见的数据范围。
#[derive(Debug, Clone)]
pub struct VisibleScope {
    /// 全国可见(NATIONAL 档);为 true 时 `includes_province` 恒真,不看 provinces。
    pub nationwide: bool,
    /// 可见省份列表。空 vec 表示无可见省(非全国时即无可见范围)。
    pub provinces: Vec<String>,
    /// 可见城市列表。空 vec 表示"不限市"(全国/省级)。
    pub cities: Vec<String>,
    /// 可见镇列表。空 vec 表示"不限镇"(全国/省级/市级)。
    pub towns: Vec<String>,
    /// 是否可以增删改。各档在自己范围内都能写,保留字段供将来扩展只读角色。
    pub can_write: bool,
    /// 进入 tab 时是否跳过省份列表直接进入详情(省级及以下锁定本省)。
    /// 前端 tab 跳级预留:VisibleScope 暂不序列化,当前后端不读,留待 UI 接线。
    #[allow(dead_code)]
    pub skip_province_list: bool,
    /// 进入 tab 时是否跳过市列表(市级及以下锁定本市)。
    #[allow(dead_code)]
    pub skip_city_list: bool,
    /// 进入 tab 时是否跳过镇列表(镇级锁定本镇)。
    #[allow(dead_code)]
    pub skip_town_list: bool,
    /// 锁定的省名称(省级及以下进入时自动填)。
    pub locked_province_name: Option<String>,
    /// 锁定的市名称(市级及以下进入时自动填)。
    pub locked_city_name: Option<String>,
    /// 锁定的镇名称(镇级进入时自动填)。
    pub locked_town_name: Option<String>,
}

impl VisibleScope {
    /// 全国:不限省/市/镇,可写。
    pub fn national() -> Self {
        Self {
            nationwide: true,
            provinces: vec![],
            cities: vec![],
            towns: vec![],
            can_write: true,
            skip_province_list: false,
            skip_city_list: false,
            skip_town_list: false,
            locked_province_name: None,
            locked_city_name: None,
            locked_town_name: None,
        }
    }

    /// 省级:只看本省,所有市,可写。
    pub fn province(province_name: String) -> Self {
        Self {
            nationwide: false,
            provinces: vec![province_name.clone()],
            cities: vec![],
            towns: vec![],
            can_write: true,
            skip_province_list: true,
            skip_city_list: false,
            skip_town_list: false,
            locked_province_name: Some(province_name),
            locked_city_name: None,
            locked_town_name: None,
        }
    }

    /// 市级:只看本市,所有镇,可写。
    pub fn city(province_name: String, city_name: String) -> Self {
        Self {
            nationwide: false,
            provinces: vec![province_name.clone()],
            cities: vec![city_name.clone()],
            towns: vec![],
            can_write: true,
            skip_province_list: true,
            skip_city_list: true,
            skip_town_list: false,
            locked_province_name: Some(province_name),
            locked_city_name: Some(city_name),
            locked_town_name: None,
        }
    }

    /// 镇级:只看本镇,可写。
    pub fn town(province_name: String, city_name: String, town_name: String) -> Self {
        Self {
            nationwide: false,
            provinces: vec![province_name.clone()],
            cities: vec![city_name.clone()],
            towns: vec![town_name.clone()],
            can_write: true,
            skip_province_list: true,
            skip_city_list: true,
            skip_town_list: true,
            locked_province_name: Some(province_name),
            locked_city_name: Some(city_name),
            locked_town_name: Some(town_name),
        }
    }

    /// 空范围。缺省域代表登录投影错误,调用方应优先拒绝请求。
    pub fn empty() -> Self {
        Self {
            nationwide: false,
            provinces: vec![],
            cities: vec![],
            towns: vec![],
            can_write: false,
            skip_province_list: false,
            skip_city_list: false,
            skip_town_list: false,
            locked_province_name: None,
            locked_city_name: None,
            locked_town_name: None,
        }
    }

    /// 判断某省是否在范围内。全国范围恒真。
    pub fn includes_province(&self, province: &str) -> bool {
        self.nationwide || self.provinces.iter().any(|p| p == province)
    }

    /// 判断某市是否在范围内。cities 为空时视为"不限市"。
    pub fn includes_city(&self, city: &str) -> bool {
        self.cities.is_empty() || self.cities.iter().any(|c| c == city)
    }

    /// 判断某镇是否在范围内。两种"不限"放行:scope 的 towns 为空(全国/省/市档)、
    /// 或记录本身无镇维度(town 空,如手动创建机构 town_code 恒空)。镇级管理员因此只排除
    /// "明确标记在其他镇"的记录(对账目录里的镇级机构),不会被无镇维度记录误锁。
    pub fn includes_town(&self, town: &str) -> bool {
        self.towns.is_empty() || town.trim().is_empty() || self.towns.iter().any(|t| t == town)
    }
}

/// 根据登录管理员上下文派生 VisibleScope。
///
/// Tier1 创世注册局(FRG)的 admin_level 虽为 NATIONAL,但其管理员按省分区
/// (每节点单省,省作用域来自节点 env / 链上省组),故先于 admin_level 经 `is_tier1_registry`
/// 谓词特判为省级范围。其余机构按 admin_level 派生;私权法人/非法人无层级,暂沿用本市范围。
/// 任一档缺必要 scope 字段返回空范围,不制造伪行政区参与查询。
pub fn get_visible_scope(ctx: &AdminAuthContext) -> VisibleScope {
    if crate::core::chain_runtime::is_tier1_registry(&ctx.institution_code) {
        return ctx
            .scope_province_name
            .clone()
            .map(VisibleScope::province)
            .unwrap_or_else(VisibleScope::empty);
    }
    match ctx.admin_level.as_deref() {
        Some("NATIONAL") => VisibleScope::national(),
        Some("PROVINCE") => ctx
            .scope_province_name
            .clone()
            .map(VisibleScope::province)
            .unwrap_or_else(VisibleScope::empty),
        Some("CITY") => scope_city_or_empty(ctx),
        Some("TOWN") => {
            let (Some(province), Some(city), Some(town)) = (
                ctx.scope_province_name.clone(),
                ctx.scope_city_name.clone(),
                ctx.scope_town_name.clone(),
            ) else {
                return VisibleScope::empty();
            };
            VisibleScope::town(province, city, town)
        }
        // 私权法人/非法人(无 admin_level):暂沿用本市范围;精确"仅本机构"范围留 admin 泛化卡。
        None => scope_city_or_empty(ctx),
        // 防御性兜底:admin_level_label_for 只产出 NATIONAL/PROVINCE/CITY/TOWN 或 None(均已覆盖),
        // 出现未知层级一律 fail-closed 空范围。
        _ => VisibleScope::empty(),
    }
}

fn scope_city_or_empty(ctx: &AdminAuthContext) -> VisibleScope {
    let (Some(province), Some(city)) =
        (ctx.scope_province_name.clone(), ctx.scope_city_name.clone())
    else {
        return VisibleScope::empty();
    };
    VisibleScope::city(province, city)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn national_sees_every_province() {
        let s = VisibleScope::national();
        assert!(s.includes_province("安徽省"));
        assert!(s.includes_province("广东省"));
        assert!(s.includes_city("合肥市"));
        assert!(s.includes_town("任意镇"));
    }

    #[test]
    fn province_limited_to_own_province_all_cities() {
        let s = VisibleScope::province("安徽省".to_string());
        assert!(s.includes_province("安徽省"));
        assert!(!s.includes_province("广东省"));
        assert!(s.includes_city("合肥市"));
        assert!(s.includes_town("任意镇"));
        assert!(s.skip_province_list);
        assert!(!s.skip_city_list);
    }

    #[test]
    fn city_limited_to_own_city_all_towns() {
        let s = VisibleScope::city("安徽省".to_string(), "合肥市".to_string());
        assert!(s.includes_province("安徽省"));
        assert!(s.includes_city("合肥市"));
        assert!(!s.includes_city("芜湖市"));
        assert!(s.includes_town("任意镇"));
        assert!(s.skip_city_list);
        assert!(!s.skip_town_list);
    }

    #[test]
    fn town_limited_to_own_town() {
        let s = VisibleScope::town(
            "安徽省".to_string(),
            "合肥市".to_string(),
            "三十岗镇".to_string(),
        );
        assert!(s.includes_province("安徽省"));
        assert!(s.includes_city("合肥市"));
        assert!(s.includes_town("三十岗镇"));
        assert!(!s.includes_town("董铺镇"));
        // 无镇维度记录(town 空,如手动创建机构)对镇级管理员仍可见,不被误锁。
        assert!(s.includes_town(""));
        assert!(s.skip_town_list);
    }

    #[test]
    fn empty_scope_matches_nothing() {
        let s = VisibleScope::empty();
        assert!(!s.includes_province("安徽省"));
        assert!(!s.can_write);
    }

    fn ctx(
        institution_code: &str,
        admin_level: Option<&str>,
        province: Option<&str>,
        city: Option<&str>,
        town: Option<&str>,
    ) -> AdminAuthContext {
        AdminAuthContext {
            account_id: "0x1111111111111111111111111111111111111111111111111111111111111111"
                .to_string(),
            institution_cid_number: "LN001-FRG0G-000000001-2026".to_string(),
            institution_code: institution_code.to_string(),
            admin_level: admin_level.map(str::to_string),
            family_name: "管理".to_string(),
            given_name: "员".to_string(),
            scope_province_name: province.map(str::to_string),
            scope_city_name: city.map(str::to_string),
            scope_town_name: town.map(str::to_string),
            cid_short_name: None,
        }
    }

    #[test]
    fn frg_is_province_scoped_despite_national_level() {
        // 安全关键:FRG 码 admin_level 为 NATIONAL,但必须特判为省级,绝不给全国范围。
        let s = get_visible_scope(&ctx("FRG", Some("NATIONAL"), Some("安徽省"), None, None));
        assert!(!s.nationwide);
        assert!(s.includes_province("安徽省"));
        assert!(!s.includes_province("广东省"));
    }

    #[test]
    fn frg_without_province_fails_closed() {
        let s = get_visible_scope(&ctx("FRG", Some("NATIONAL"), None, None, None));
        assert!(!s.includes_province("安徽省"));
    }

    #[test]
    fn national_ministry_sees_nationwide() {
        let s = get_visible_scope(&ctx("MFA", Some("NATIONAL"), None, None, None));
        assert!(s.nationwide);
        assert!(s.includes_province("广东省"));
    }

    #[test]
    fn province_level_limited_to_own_province() {
        let s = get_visible_scope(&ctx("PGV", Some("PROVINCE"), Some("安徽省"), None, None));
        assert!(s.includes_province("安徽省"));
        assert!(!s.includes_province("广东省"));
        assert!(s.includes_city("合肥市"));
    }

    #[test]
    fn city_level_limited_to_own_city() {
        let s = get_visible_scope(&ctx(
            "CGOV",
            Some("CITY"),
            Some("安徽省"),
            Some("合肥市"),
            None,
        ));
        assert!(s.includes_city("合肥市"));
        assert!(!s.includes_city("芜湖市"));
    }

    #[test]
    fn town_level_limited_to_own_town() {
        let s = get_visible_scope(&ctx(
            "TGOV",
            Some("TOWN"),
            Some("安徽省"),
            Some("合肥市"),
            Some("三十岗镇"),
        ));
        assert!(s.includes_town("三十岗镇"));
        assert!(!s.includes_town("董铺镇"));
    }

    #[test]
    fn town_level_missing_town_fails_closed() {
        let s = get_visible_scope(&ctx(
            "TGOV",
            Some("TOWN"),
            Some("安徽省"),
            Some("合肥市"),
            None,
        ));
        assert!(!s.includes_province("安徽省"));
    }

    #[test]
    fn private_none_level_falls_back_to_city() {
        // 私权法人/非法人无 admin_level:暂沿用本市范围(决策 1①)。
        let s = get_visible_scope(&ctx("SFLP", None, Some("安徽省"), Some("合肥市"), None));
        assert!(s.includes_city("合肥市"));
        assert!(!s.includes_city("芜湖市"));
    }
}

//! 原所属模块回归，断言与输入保持。
use super::*;

#[test]
fn split_province_city_reads_foundation_scope() {
    // 基金会机构 CID → 贵州(GZ)/绥阳(018);创世公民程伟居住地即取此。
    let scope = match split_province_city(CITIZENCHAIN_FOUNDATION.cid_number) {
        Ok(value) => value,
        Err(error) => panic!("foundation scope should decode: {error}"),
    };
    assert_eq!(scope, ("GZ".to_string(), "018".to_string()));
}

#[test]
fn split_province_city_rejects_person_cid() {
    // 程伟为人主体 CN 号,去地域化、R5 不载省市 → fail-closed。
    // 播种须改取基金会省市,绝不对程伟号调用本函数(否则误把号段读成区划)。
    assert!(split_province_city(LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER).is_err());
}

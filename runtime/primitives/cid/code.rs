//! 国家码、省码与 CID 机构码常量真源。
//! 国家/省码为 2 位 ASCII Base36 字符集;机构码为 3~4 位 ASCII Base36 字符集,链上用 `[u8; 4]`。

/// 国家码。
pub type CountryCode = [u8; 2];

/// 省行政区码。
pub type ProvinceCode = [u8; 2];

/// 机构码链上表示。
pub type InstitutionCode = [u8; 4];

/// 国家代码元数据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CountryCodeInfo {
    pub country_code: CountryCode,
    pub country_full_name: &'static str,
    pub country_full_name_en: &'static str,
    pub country_short_name: &'static str,
    pub country_short_name_en: &'static str,
}

/// 省行政区代码元数据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProvinceCodeInfo {
    pub province_code: ProvinceCode,
    pub province_name: &'static str,
}

/// 机构码元数据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstitutionCodeInfo {
    pub institution_code: InstitutionCode,
    pub institution_code_text: &'static str,
    pub institution_code_label: &'static str,
}

/// 机构码所属行政层。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminLevel {
    /// 国家码 2位 ASCII Base36 字符集。
    National,
    /// 省行政区 2位 ASCII Base36 字符集。
    Province,
    /// 市行政区 4位 ASCII Base36 字符集 市行政区码3位 000-999。
    City,
    /// 镇行政区 4位 ASCII Base36 字符集 镇行政区码3位 001-999。
    Town,
}

/// 机构码盈利策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfitPolicy {
    /// 固定非盈利。
    NonProfit,
    /// 固定盈利。
    Profit,
    /// 按实例可变。
    Variable,
    /// 继承父级法人盈利属性。
    InheritParent,
}

/// 国家代码:中华民族联邦共和国。
pub const COUNTRY_CN: CountryCode = *b"CN";

/// 国家代码信息。
pub const COUNTRY_CN_INFO: CountryCodeInfo = CountryCodeInfo {
    country_code: COUNTRY_CN,
    country_full_name: "中华民族联邦共和国",
    country_full_name_en: "Federal Republic of the Chinese Nation",
    country_short_name: "中华联邦",
    country_short_name_en: "Chinese Federation",
};

/// 从文本机构码构造链上字节表示。
pub const fn code_bytes(s: &str) -> InstitutionCode {
    let b = s.as_bytes();
    let mut out = [0u8; 4];
    let mut i = 0;
    while i < b.len() && i < 4 {
        out[i] = b[i];
        i += 1;
    }
    out
}

/// 省行政区代码表。
pub const PROVINCE_CODE_INFOS: [ProvinceCodeInfo; 43] = [
    ProvinceCodeInfo {
        province_code: *b"ZS",
        province_name: "中枢省",
    },
    ProvinceCodeInfo {
        province_code: *b"LN",
        province_name: "岭南省",
    },
    ProvinceCodeInfo {
        province_code: *b"GD",
        province_name: "广东省",
    },
    ProvinceCodeInfo {
        province_code: *b"GX",
        province_name: "广西省",
    },
    ProvinceCodeInfo {
        province_code: *b"FJ",
        province_name: "福建省",
    },
    ProvinceCodeInfo {
        province_code: *b"HN",
        province_name: "海南省",
    },
    ProvinceCodeInfo {
        province_code: *b"YN",
        province_name: "云南省",
    },
    ProvinceCodeInfo {
        province_code: *b"GZ",
        province_name: "贵州省",
    },
    ProvinceCodeInfo {
        province_code: *b"HU",
        province_name: "湖南省",
    },
    ProvinceCodeInfo {
        province_code: *b"JX",
        province_name: "江西省",
    },
    ProvinceCodeInfo {
        province_code: *b"ZJ",
        province_name: "浙江省",
    },
    ProvinceCodeInfo {
        province_code: *b"JS",
        province_name: "江苏省",
    },
    ProvinceCodeInfo {
        province_code: *b"SD",
        province_name: "山东省",
    },
    ProvinceCodeInfo {
        province_code: *b"SX",
        province_name: "山西省",
    },
    ProvinceCodeInfo {
        province_code: *b"HE",
        province_name: "河南省",
    },
    ProvinceCodeInfo {
        province_code: *b"HB",
        province_name: "河北省",
    },
    ProvinceCodeInfo {
        province_code: *b"HI",
        province_name: "湖北省",
    },
    ProvinceCodeInfo {
        province_code: *b"SI",
        province_name: "陕西省",
    },
    ProvinceCodeInfo {
        province_code: *b"CQ",
        province_name: "重庆省",
    },
    ProvinceCodeInfo {
        province_code: *b"SC",
        province_name: "四川省",
    },
    ProvinceCodeInfo {
        province_code: *b"GS",
        province_name: "甘肃省",
    },
    ProvinceCodeInfo {
        province_code: *b"BP",
        province_name: "北平省",
    },
    ProvinceCodeInfo {
        province_code: *b"HA",
        province_name: "海滨省",
    },
    ProvinceCodeInfo {
        province_code: *b"SJ",
        province_name: "松江省",
    },
    ProvinceCodeInfo {
        province_code: *b"LJ",
        province_name: "龙江省",
    },
    ProvinceCodeInfo {
        province_code: *b"JL",
        province_name: "吉林省",
    },
    ProvinceCodeInfo {
        province_code: *b"LI",
        province_name: "辽宁省",
    },
    ProvinceCodeInfo {
        province_code: *b"NX",
        province_name: "宁夏省",
    },
    ProvinceCodeInfo {
        province_code: *b"QH",
        province_name: "青海省",
    },
    ProvinceCodeInfo {
        province_code: *b"AH",
        province_name: "安徽省",
    },
    ProvinceCodeInfo {
        province_code: *b"TW",
        province_name: "台湾省",
    },
    ProvinceCodeInfo {
        province_code: *b"XZ",
        province_name: "西藏省",
    },
    ProvinceCodeInfo {
        province_code: *b"XJ",
        province_name: "新疆省",
    },
    ProvinceCodeInfo {
        province_code: *b"XK",
        province_name: "西康省",
    },
    ProvinceCodeInfo {
        province_code: *b"AL",
        province_name: "阿里省",
    },
    ProvinceCodeInfo {
        province_code: *b"CL",
        province_name: "葱岭省",
    },
    ProvinceCodeInfo {
        province_code: *b"YL",
        province_name: "伊犁省",
    },
    ProvinceCodeInfo {
        province_code: *b"HX",
        province_name: "河西省",
    },
    ProvinceCodeInfo {
        province_code: *b"KL",
        province_name: "昆仑省",
    },
    ProvinceCodeInfo {
        province_code: *b"HT",
        province_name: "河套省",
    },
    ProvinceCodeInfo {
        province_code: *b"RH",
        province_name: "热河省",
    },
    ProvinceCodeInfo {
        province_code: *b"XA",
        province_name: "兴安省",
    },
    ProvinceCodeInfo {
        province_code: *b"HJ",
        province_name: "合江省",
    },
];

const fn province_codes_from_infos() -> [ProvinceCode; 43] {
    let mut out = [[0u8; 2]; 43];
    let mut i = 0;
    while i < PROVINCE_CODE_INFOS.len() {
        out[i] = PROVINCE_CODE_INFOS[i].province_code;
        i += 1;
    }
    out
}

/// 全部省行政区代码,顺序与 `PROVINCE_CODE_INFOS` 一致。
pub const PROVINCE_CODES: [ProvinceCode; 43] = province_codes_from_infos();

/// 国家码转文本。
pub fn country_code_text(code: &CountryCode) -> Option<&str> {
    core::str::from_utf8(code).ok()
}

/// 省行政区码转文本。
pub fn province_code_text(code: &ProvinceCode) -> Option<&'static str> {
    let matched = PROVINCE_CODE_INFOS
        .iter()
        .find(|info| info.province_code.eq_ignore_ascii_case(code))?;
    core::str::from_utf8(&matched.province_code).ok()
}

/// 省行政区名称转两位省码。
pub fn province_code_by_name(province_name: &str) -> Option<ProvinceCode> {
    PROVINCE_CODE_INFOS
        .iter()
        .find(|info| info.province_name == province_name.trim())
        .map(|info| info.province_code)
}

/// 两位省码转省行政区名称。
pub fn province_name_by_code(code: &ProvinceCode) -> Option<&'static str> {
    PROVINCE_CODE_INFOS
        .iter()
        .find(|info| info.province_code.eq_ignore_ascii_case(code))
        .map(|info| info.province_name)
}
// 机构码清单按 A-I 九组排列。
/// 国家储委会。
pub const NRC: InstitutionCode = *b"NRC\0";
/// 省公民储备委员会。
pub const PRC: InstitutionCode = *b"PRC\0";
/// 省公民储备银行。
pub const PRB: InstitutionCode = *b"PRB\0";
/// 联邦注册局。
pub const FRG: InstitutionCode = *b"FRG\0";
/// 国家司法院。
pub const NJD: InstitutionCode = *b"NJD\0";
/// 总统府。
pub const PRS: InstitutionCode = *b"PRS\0";
/// 联邦安全局(总统府下属;唯一持有「联邦公民安全基金」协议账户)。
pub const FSC: InstitutionCode = *b"FSC\0";
/// 国家立法院。
pub const NLG: InstitutionCode = *b"NLG\0";
/// 国家参议会。
pub const NSN: InstitutionCode = *b"NSN\0";
/// 国家众议会。
pub const NRP: InstitutionCode = *b"NRP\0";
/// 国家监察院。
pub const NSP: InstitutionCode = *b"NSP\0";
/// 国家教委会。
pub const NED: InstitutionCode = *b"NED\0";

/// 个人多签账户,不发 CID 号。
pub const PMUL: InstitutionCode = *b"PMUL";

/// 私法人股份公司(清算行资格机构)。
pub const SFGF: InstitutionCode = *b"SFGF";

/// 非法人组织。唯一必挂法人父级(盈利策略 `InheritParent`)的机构码;
/// 父级为 `SFGF` 时即股份公司的非法人分支机构,同样具备清算行资格。
pub const UNIN: InstitutionCode = *b"UNIN";

/// 全部 104 个机构码信息,按 A-I 九组排列。
pub const INSTITUTION_CODE_INFOS: [InstitutionCodeInfo; 104] = [
    // A 国家码单体(38,3 位,公法人,非盈利)
    InstitutionCodeInfo {
        institution_code: PRS,
        institution_code_text: "PRS",
        institution_code_label: "总统府",
    },
    InstitutionCodeInfo {
        institution_code: *b"FSC\0",
        institution_code_text: "FSC",
        institution_code_label: "联邦安全局",
    },
    InstitutionCodeInfo {
        institution_code: *b"FIB\0",
        institution_code_text: "FIB",
        institution_code_label: "联邦情报局",
    },
    InstitutionCodeInfo {
        institution_code: *b"FSS\0",
        institution_code_text: "FSS",
        institution_code_label: "联邦特勤局",
    },
    InstitutionCodeInfo {
        institution_code: *b"FPR\0",
        institution_code_text: "FPR",
        institution_code_label: "联邦人事局",
    },
    InstitutionCodeInfo {
        institution_code: FRG,
        institution_code_text: "FRG",
        institution_code_label: "联邦注册局",
    },
    InstitutionCodeInfo {
        institution_code: *b"MFA\0",
        institution_code_text: "MFA",
        institution_code_label: "外交部",
    },
    InstitutionCodeInfo {
        institution_code: *b"MDF\0",
        institution_code_text: "MDF",
        institution_code_label: "国防部",
    },
    // 国防部下属军政部门与作战/军种司令部,均属国家公权机构码。
    InstitutionCodeInfo {
        institution_code: *b"ARM\0",
        institution_code_text: "ARM",
        institution_code_label: "陆军部",
    },
    InstitutionCodeInfo {
        institution_code: *b"NAV\0",
        institution_code_text: "NAV",
        institution_code_label: "海军部",
    },
    InstitutionCodeInfo {
        institution_code: *b"AIR\0",
        institution_code_text: "AIR",
        institution_code_label: "空军部",
    },
    InstitutionCodeInfo {
        institution_code: *b"SPF\0",
        institution_code_text: "SPF",
        institution_code_label: "天军部",
    },
    InstitutionCodeInfo {
        institution_code: *b"JOS\0",
        institution_code_text: "JOS",
        institution_code_label: "联合作战参谋部",
    },
    InstitutionCodeInfo {
        institution_code: *b"ARC\0",
        institution_code_text: "ARC",
        institution_code_label: "陆军司令部",
    },
    InstitutionCodeInfo {
        institution_code: *b"NVC\0",
        institution_code_text: "NVC",
        institution_code_label: "海军司令部",
    },
    InstitutionCodeInfo {
        institution_code: *b"AFC\0",
        institution_code_text: "AFC",
        institution_code_label: "空军司令部",
    },
    InstitutionCodeInfo {
        institution_code: *b"SFC\0",
        institution_code_text: "SFC",
        institution_code_label: "天军司令部",
    },
    InstitutionCodeInfo {
        institution_code: *b"MHS\0",
        institution_code_text: "MHS",
        institution_code_label: "国安部",
    },
    InstitutionCodeInfo {
        institution_code: *b"NGB\0",
        institution_code_text: "NGB",
        institution_code_label: "国民警卫局",
    },
    InstitutionCodeInfo {
        institution_code: *b"NGC\0",
        institution_code_text: "NGC",
        institution_code_label: "国民警卫队司令部",
    },
    InstitutionCodeInfo {
        institution_code: *b"MCW\0",
        institution_code_text: "MCW",
        institution_code_label: "民生部",
    },
    InstitutionCodeInfo {
        institution_code: *b"FDA\0",
        institution_code_text: "FDA",
        institution_code_label: "食品药品监管局",
    },
    InstitutionCodeInfo {
        institution_code: *b"MHU\0",
        institution_code_text: "MHU",
        institution_code_label: "住建部",
    },
    InstitutionCodeInfo {
        institution_code: *b"MAG\0",
        institution_code_text: "MAG",
        institution_code_label: "农业部",
    },
    InstitutionCodeInfo {
        institution_code: *b"MCM\0",
        institution_code_text: "MCM",
        institution_code_label: "商贸部",
    },
    InstitutionCodeInfo {
        institution_code: *b"MFT\0",
        institution_code_text: "MFT",
        institution_code_label: "财税部",
    },
    InstitutionCodeInfo {
        institution_code: *b"MEN\0",
        institution_code_text: "MEN",
        institution_code_label: "能源部",
    },
    InstitutionCodeInfo {
        institution_code: *b"MTR\0",
        institution_code_text: "MTR",
        institution_code_label: "交通部",
    },
    InstitutionCodeInfo {
        institution_code: NLG,
        institution_code_text: "NLG",
        institution_code_label: "国家立法院",
    },
    InstitutionCodeInfo {
        institution_code: NSN,
        institution_code_text: "NSN",
        institution_code_label: "国家参议会",
    },
    InstitutionCodeInfo {
        institution_code: NRP,
        institution_code_text: "NRP",
        institution_code_label: "国家众议会",
    },
    InstitutionCodeInfo {
        institution_code: *b"NJD\0",
        institution_code_text: "NJD",
        institution_code_label: "国家司法院",
    },
    InstitutionCodeInfo {
        institution_code: NSP,
        institution_code_text: "NSP",
        institution_code_label: "国家监察院",
    },
    InstitutionCodeInfo {
        institution_code: *b"FAC\0",
        institution_code_text: "FAC",
        institution_code_label: "联邦廉政署",
    },
    InstitutionCodeInfo {
        institution_code: *b"FAU\0",
        institution_code_text: "FAU",
        institution_code_label: "联邦审计署",
    },
    InstitutionCodeInfo {
        institution_code: *b"FIV\0",
        institution_code_text: "FIV",
        institution_code_label: "联邦调查署",
    },
    InstitutionCodeInfo {
        institution_code: NED,
        institution_code_text: "NED",
        institution_code_label: "国家教委会",
    },
    InstitutionCodeInfo {
        institution_code: *b"NRC\0",
        institution_code_text: "NRC",
        institution_code_label: "国家储委会",
    },
    // B 省类型(17,3 位,43 省共用,R5 省码区分实例,非盈利)
    InstitutionCodeInfo {
        institution_code: *b"PGV\0",
        institution_code_text: "PGV",
        institution_code_label: "省政府",
    },
    InstitutionCodeInfo {
        institution_code: *b"PLG\0",
        institution_code_text: "PLG",
        institution_code_label: "省立法院",
    },
    InstitutionCodeInfo {
        institution_code: *b"PSN\0",
        institution_code_text: "PSN",
        institution_code_label: "省参议会",
    },
    InstitutionCodeInfo {
        institution_code: *b"PRP\0",
        institution_code_text: "PRP",
        institution_code_label: "省众议会",
    },
    InstitutionCodeInfo {
        institution_code: *b"PJD\0",
        institution_code_text: "PJD",
        institution_code_label: "省司法院",
    },
    InstitutionCodeInfo {
        institution_code: *b"PSP\0",
        institution_code_text: "PSP",
        institution_code_label: "省监察院",
    },
    InstitutionCodeInfo {
        institution_code: *b"PRC\0",
        institution_code_text: "PRC",
        institution_code_label: "省储委会",
    },
    InstitutionCodeInfo {
        institution_code: *b"PRB\0",
        institution_code_text: "PRB",
        institution_code_label: "省储行",
    },
    InstitutionCodeInfo {
        institution_code: *b"PDF\0",
        institution_code_text: "PDF",
        institution_code_label: "省国防厅",
    },
    InstitutionCodeInfo {
        institution_code: *b"PHS\0",
        institution_code_text: "PHS",
        institution_code_label: "省国安厅",
    },
    InstitutionCodeInfo {
        institution_code: *b"PCW\0",
        institution_code_text: "PCW",
        institution_code_label: "省民生厅",
    },
    InstitutionCodeInfo {
        institution_code: *b"PHU\0",
        institution_code_text: "PHU",
        institution_code_label: "省住建厅",
    },
    InstitutionCodeInfo {
        institution_code: *b"PAG\0",
        institution_code_text: "PAG",
        institution_code_label: "省农业厅",
    },
    InstitutionCodeInfo {
        institution_code: *b"PCM\0",
        institution_code_text: "PCM",
        institution_code_label: "省商贸厅",
    },
    InstitutionCodeInfo {
        institution_code: *b"PFT\0",
        institution_code_text: "PFT",
        institution_code_label: "省财税厅",
    },
    InstitutionCodeInfo {
        institution_code: *b"PEN\0",
        institution_code_text: "PEN",
        institution_code_label: "省能源厅",
    },
    InstitutionCodeInfo {
        institution_code: *b"PTR\0",
        institution_code_text: "PTR",
        institution_code_label: "省交通厅",
    },
    // C 市类型(17,4 位,非盈利)
    InstitutionCodeInfo {
        institution_code: *b"CGOV",
        institution_code_text: "CGOV",
        institution_code_label: "市政府",
    },
    InstitutionCodeInfo {
        institution_code: *b"CLEG",
        institution_code_text: "CLEG",
        institution_code_label: "市立法会",
    },
    InstitutionCodeInfo {
        institution_code: *b"CSUP",
        institution_code_text: "CSUP",
        institution_code_label: "市监察院",
    },
    InstitutionCodeInfo {
        institution_code: *b"CJUD",
        institution_code_text: "CJUD",
        institution_code_label: "市司法院",
    },
    InstitutionCodeInfo {
        institution_code: *b"CEDU",
        institution_code_text: "CEDU",
        institution_code_label: "市教委会",
    },
    InstitutionCodeInfo {
        institution_code: *b"CSLF",
        institution_code_text: "CSLF",
        institution_code_label: "市自治会",
    },
    InstitutionCodeInfo {
        institution_code: *b"CDEF",
        institution_code_text: "CDEF",
        institution_code_label: "市国防局",
    },
    InstitutionCodeInfo {
        institution_code: *b"CHSC",
        institution_code_text: "CHSC",
        institution_code_label: "市国安局",
    },
    InstitutionCodeInfo {
        institution_code: *b"CCWF",
        institution_code_text: "CCWF",
        institution_code_label: "市民生局",
    },
    InstitutionCodeInfo {
        institution_code: *b"CHUD",
        institution_code_text: "CHUD",
        institution_code_label: "市住建局",
    },
    InstitutionCodeInfo {
        institution_code: *b"CAGR",
        institution_code_text: "CAGR",
        institution_code_label: "市农业局",
    },
    InstitutionCodeInfo {
        institution_code: *b"CCOM",
        institution_code_text: "CCOM",
        institution_code_label: "市商贸局",
    },
    InstitutionCodeInfo {
        institution_code: *b"CFIN",
        institution_code_text: "CFIN",
        institution_code_label: "市财税局",
    },
    InstitutionCodeInfo {
        institution_code: *b"CENR",
        institution_code_text: "CENR",
        institution_code_label: "市能源局",
    },
    InstitutionCodeInfo {
        institution_code: *b"CTRN",
        institution_code_text: "CTRN",
        institution_code_label: "市交通局",
    },
    InstitutionCodeInfo {
        institution_code: *b"CREG",
        institution_code_text: "CREG",
        institution_code_label: "市注册局",
    },
    InstitutionCodeInfo {
        institution_code: *b"CPOL",
        institution_code_text: "CPOL",
        institution_code_label: "市公安局",
    },
    // D 镇类型(14,4 位,非盈利)
    InstitutionCodeInfo {
        institution_code: *b"TGOV",
        institution_code_text: "TGOV",
        institution_code_label: "镇政府",
    },
    InstitutionCodeInfo {
        institution_code: *b"TCWF",
        institution_code_text: "TCWF",
        institution_code_label: "镇民生科",
    },
    InstitutionCodeInfo {
        institution_code: *b"THUD",
        institution_code_text: "THUD",
        institution_code_label: "镇住建科",
    },
    InstitutionCodeInfo {
        institution_code: *b"TAGR",
        institution_code_text: "TAGR",
        institution_code_label: "镇农业科",
    },
    InstitutionCodeInfo {
        institution_code: *b"TFIN",
        institution_code_text: "TFIN",
        institution_code_label: "镇财税科",
    },
    InstitutionCodeInfo {
        institution_code: *b"TDEF",
        institution_code_text: "TDEF",
        institution_code_label: "镇国防科",
    },
    InstitutionCodeInfo {
        institution_code: *b"THSC",
        institution_code_text: "THSC",
        institution_code_label: "镇国安科",
    },
    InstitutionCodeInfo {
        institution_code: *b"TCOM",
        institution_code_text: "TCOM",
        institution_code_label: "镇商贸科",
    },
    InstitutionCodeInfo {
        institution_code: *b"TENR",
        institution_code_text: "TENR",
        institution_code_label: "镇能源科",
    },
    InstitutionCodeInfo {
        institution_code: *b"TTRN",
        institution_code_text: "TTRN",
        institution_code_label: "镇交通科",
    },
    InstitutionCodeInfo {
        institution_code: *b"TPOL",
        institution_code_text: "TPOL",
        institution_code_label: "镇公安科",
    },
    InstitutionCodeInfo {
        institution_code: *b"TSLF",
        institution_code_text: "TSLF",
        institution_code_label: "镇自治会",
    },
    InstitutionCodeInfo {
        institution_code: *b"TSUP",
        institution_code_text: "TSUP",
        institution_code_label: "镇监察院",
    },
    InstitutionCodeInfo {
        institution_code: *b"TJUD",
        institution_code_text: "TJUD",
        institution_code_label: "镇司法院",
    },
    // E 私权机构(7,4 位)
    InstitutionCodeInfo {
        institution_code: *b"SFGT",
        institution_code_text: "SFGT",
        institution_code_label: "个体经营",
    },
    InstitutionCodeInfo {
        institution_code: *b"SFGP",
        institution_code_text: "SFGP",
        institution_code_label: "无限合伙",
    },
    InstitutionCodeInfo {
        institution_code: *b"SFLP",
        institution_code_text: "SFLP",
        institution_code_label: "有限合伙",
    },
    InstitutionCodeInfo {
        institution_code: *b"SFGQ",
        institution_code_text: "SFGQ",
        institution_code_label: "股权公司",
    },
    InstitutionCodeInfo {
        institution_code: *b"SFGF",
        institution_code_text: "SFGF",
        institution_code_label: "股份公司",
    },
    InstitutionCodeInfo {
        institution_code: *b"SFGY",
        institution_code_text: "SFGY",
        institution_code_label: "公益组织",
    },
    InstitutionCodeInfo {
        institution_code: *b"SFAS",
        institution_code_text: "SFAS",
        institution_code_label: "注册协会",
    },
    // F 教育学校(6:公私教大学 3 位 / 公私教中小初学 4 位)
    InstitutionCodeInfo {
        institution_code: *b"GUN\0",
        institution_code_text: "GUN",
        institution_code_label: "公立大学",
    },
    InstitutionCodeInfo {
        institution_code: *b"SUN\0",
        institution_code_text: "SUN",
        institution_code_label: "私立大学",
    },
    InstitutionCodeInfo {
        institution_code: *b"JUN\0",
        institution_code_text: "JUN",
        institution_code_label: "教会大学",
    },
    InstitutionCodeInfo {
        institution_code: *b"GSCH",
        institution_code_text: "GSCH",
        institution_code_label: "公立学校",
    },
    InstitutionCodeInfo {
        institution_code: *b"SFSC",
        institution_code_text: "SFSC",
        institution_code_label: "私立学校",
    },
    InstitutionCodeInfo {
        institution_code: *b"JSCH",
        institution_code_text: "JSCH",
        institution_code_label: "教会学校",
    },
    // G 个人主体(3,4 位)
    InstitutionCodeInfo {
        institution_code: *b"CTZN",
        institution_code_text: "CTZN",
        institution_code_label: "公民人",
    },
    InstitutionCodeInfo {
        institution_code: *b"NATP",
        institution_code_text: "NATP",
        institution_code_label: "自然人",
    },
    InstitutionCodeInfo {
        institution_code: *b"SMTP",
        institution_code_text: "SMTP",
        institution_code_label: "智能人",
    },
    // H 非法人组织(1,4 位)
    InstitutionCodeInfo {
        institution_code: *b"UNIN",
        institution_code_text: "UNIN",
        institution_code_label: "非法人组织",
    },
    // I 个人多签(1,4 位,不发号)
    InstitutionCodeInfo {
        institution_code: *b"PMUL",
        institution_code_text: "PMUL",
        institution_code_label: "个人多签",
    },
];

const fn institution_codes_from_infos() -> [InstitutionCode; 104] {
    let mut out = [[0u8; 4]; 104];
    let mut i = 0;
    while i < INSTITUTION_CODE_INFOS.len() {
        out[i] = INSTITUTION_CODE_INFOS[i].institution_code;
        i += 1;
    }
    out
}

/// 全部 104 个机构码,顺序与 `INSTITUTION_CODE_INFOS` 一致。
pub const ALL_CODES: [InstitutionCode; 104] = institution_codes_from_infos();

fn institution_info(code: &InstitutionCode) -> Option<&'static InstitutionCodeInfo> {
    INSTITUTION_CODE_INFOS
        .iter()
        .find(|info| info.institution_code == *code)
}

/// 机构码字节转 3/4 字符文本。
pub fn institution_code_text(code: &InstitutionCode) -> Option<&'static str> {
    institution_info(code).map(|info| info.institution_code_text)
}

/// 机构码对应的中文标签。
pub fn institution_code_label(code: &InstitutionCode) -> Option<&'static str> {
    institution_info(code).map(|info| info.institution_code_label)
}

/// 解析机构码:接受 3/4 字符机构码或机构码中文标签。
pub fn institution_code_from_str(value: &str) -> Option<InstitutionCode> {
    let v = value.trim();
    INSTITUTION_CODE_INFOS
        .iter()
        .find(|info| info.institution_code_text == v || info.institution_code_label == v)
        .map(|info| info.institution_code)
}

/// 机构码字符长度。
pub fn institution_code_len(code: &InstitutionCode) -> Option<usize> {
    institution_code_text(code).map(str::len)
}

/// 是否为 3 字符码。
pub fn is_three_char_code(code: &InstitutionCode) -> bool {
    institution_code_len(code) == Some(3)
}

/// 从 CID 号第二段解析机构码。
pub fn institution_code_from_cid_number(cid_number: &str) -> Option<InstitutionCode> {
    let seg2 = cid_number.split('-').nth(1)?;
    let b = seg2.as_bytes();
    if b.len() < 4 {
        return None;
    }
    let code_len = if b[3].is_ascii_alphabetic() { 4 } else { 3 };
    let mut out = [0u8; 4];
    let mut i = 0;
    while i < code_len {
        out[i] = b[i];
        i += 1;
    }
    Some(out)
}

fn text_matches(code: &InstitutionCode, values: &[&str]) -> bool {
    let Some(text) = institution_code_text(code) else {
        return false;
    };
    values.contains(&text)
}

/// 获取机构码盈利策略。
pub fn profit_policy(code: &InstitutionCode) -> Option<ProfitPolicy> {
    let text = institution_code_text(code)?;
    let policy = match text {
        "SFGT" | "SFGP" | "SFLP" | "SFGQ" | "SFGF" | "CTZN" | "NATP" => ProfitPolicy::Profit,
        "SFAS" | "SMTP" | "SUN" | "SFSC" | "JUN" | "JSCH" => ProfitPolicy::Variable,
        "UNIN" => ProfitPolicy::InheritParent,
        _ => ProfitPolicy::NonProfit,
    };
    Some(policy)
}

/// 是否个人主体(公民 CTZN / 居民 NATP / 智能人 SMTP)。
///
/// 人主体 CID 去地域化,与机构 CID 生成方式彻底分开:R5 = CN 国家码 + 号码高 3 位、
/// N9 低 9 位(号段 12 位 = 1e12/年,不承载省/市码);机构 CID 才用省码 + 市码。
pub fn is_person_code(code: &InstitutionCode) -> bool {
    text_matches(code, &["CTZN", "NATP", "SMTP"])
}

/// 是否非法人。
pub fn is_unincorporated_code(code: &InstitutionCode) -> bool {
    text_matches(code, &["SFGT", "SFGP", "UNIN"])
}

/// 是否私法人。
pub fn is_private_legal_code(code: &InstitutionCode) -> bool {
    text_matches(
        code,
        &[
            "SFLP", "SFGQ", "SFGF", "SFGY", "SFAS", "SUN", "JUN", "SFSC", "JSCH",
        ],
    )
}

/// 机构码所属行政层。
pub fn admin_level(code: &InstitutionCode) -> Option<AdminLevel> {
    let text = institution_code_text(code)?;
    match text {
        // A 国家单体(38)
        "PRS" | "FSC" | "FIB" | "FSS" | "FPR" | "FRG" | "MFA" | "MDF" | "ARM" | "NAV" | "AIR"
        | "SPF" | "JOS" | "ARC" | "NVC" | "AFC" | "SFC" | "MHS" | "NGB" | "NGC" | "MCW" | "FDA"
        | "MHU" | "MAG" | "MCM" | "MFT" | "MEN" | "MTR" | "NLG" | "NSN" | "NRP" | "NJD" | "NSP"
        | "FAC" | "FAU" | "FIV" | "NED" | "NRC" => Some(AdminLevel::National),
        // B 省类型(17)
        "PGV" | "PLG" | "PSN" | "PRP" | "PJD" | "PSP" | "PRC" | "PRB" | "PDF" | "PHS" | "PCW"
        | "PHU" | "PAG" | "PCM" | "PFT" | "PEN" | "PTR" => Some(AdminLevel::Province),
        // C 市类型(17)
        "CGOV" | "CLEG" | "CSUP" | "CJUD" | "CEDU" | "CSLF" | "CDEF" | "CHSC" | "CCWF" | "CHUD"
        | "CAGR" | "CCOM" | "CFIN" | "CENR" | "CTRN" | "CREG" | "CPOL" => Some(AdminLevel::City),
        // D 镇类型(14)
        "TGOV" | "TCWF" | "THUD" | "TAGR" | "TFIN" | "TDEF" | "THSC" | "TCOM" | "TENR" | "TTRN"
        | "TPOL" | "TSLF" | "TSUP" | "TJUD" => Some(AdminLevel::Town),
        _ => None,
    }
}

/// 是否公法人。
pub fn is_public_legal_code(code: &InstitutionCode) -> bool {
    admin_level(code).is_some() || text_matches(code, &["GUN", "GSCH"])
}

/// 是否教育机构。
pub fn is_education_institution_code(code: &InstitutionCode) -> bool {
    text_matches(code, &["GUN", "SUN", "JUN", "GSCH", "SFSC", "JSCH"])
}

/// 是否基础教育学校。
pub fn requires_education_level(code: &InstitutionCode) -> bool {
    text_matches(code, &["GSCH", "SFSC", "JSCH"])
}

/// 是否固定治理档机构码。
pub fn is_fixed_governance_code(code: &InstitutionCode) -> bool {
    matches!(*code, NRC | PRC | PRB | FRG | NJD)
}

/// 固定治理档机构码的制度阈值。
pub fn fixed_governance_pass_threshold(code: &InstitutionCode) -> Option<u32> {
    use crate::count_const::{
        FRG_INTERNAL_THRESHOLD, NJD_INTERNAL_THRESHOLD, NRC_INTERNAL_THRESHOLD,
        PRB_INTERNAL_THRESHOLD, PRC_INTERNAL_THRESHOLD,
    };
    match *code {
        NRC => Some(NRC_INTERNAL_THRESHOLD),
        PRC => Some(PRC_INTERNAL_THRESHOLD),
        PRB => Some(PRB_INTERNAL_THRESHOLD),
        FRG => Some(FRG_INTERNAL_THRESHOLD),
        NJD => Some(NJD_INTERNAL_THRESHOLD),
        _ => None,
    }
}

/// 是否个人多签账户机构码。
pub fn is_personal_code(code: &InstitutionCode) -> bool {
    *code == PMUL
}

/// 是否机构账户机构码。
pub fn is_institution_code(code: &InstitutionCode) -> bool {
    !is_fixed_governance_code(code)
        && (is_public_legal_code(code)
            || is_private_legal_code(code)
            || is_unincorporated_code(code))
}

/// 是否注册多签动态阈值账户机构码。
pub fn is_registered_multisig_code(code: &InstitutionCode) -> bool {
    is_personal_code(code) || is_institution_code(code)
}

/// 是否内部投票支持的治理机构码。
pub fn is_valid_governance_code(code: &InstitutionCode) -> bool {
    is_fixed_governance_code(code) || is_registered_multisig_code(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn country_code_uses_constitution_name() {
        assert_eq!(country_code_text(&COUNTRY_CN), Some("CN"));
        assert_eq!(COUNTRY_CN_INFO.country_full_name, "中华民族联邦共和国");
        assert_eq!(
            COUNTRY_CN_INFO.country_full_name_en,
            "Federal Republic of the Chinese Nation"
        );
        assert_eq!(COUNTRY_CN_INFO.country_short_name, "中华联邦");
        assert_eq!(COUNTRY_CN_INFO.country_short_name_en, "Chinese Federation");
    }

    #[test]
    fn province_codes_are_two_uppercase_and_unique() {
        assert_eq!(PROVINCE_CODE_INFOS.len(), 43);
        for info in PROVINCE_CODE_INFOS {
            let text = province_code_text(&info.province_code).expect("province code ascii");
            assert_eq!(text.len(), 2);
            assert!(text.chars().all(|ch| ch.is_ascii_uppercase()));
        }
        for (i, province_code) in PROVINCE_CODES.iter().enumerate() {
            for other_province_code in PROVINCE_CODES.iter().skip(i + 1) {
                assert_ne!(province_code, other_province_code, "province duplicate");
            }
        }
        assert_eq!(province_code_by_name("广东省"), Some(*b"GD"));
        assert_eq!(province_name_by_code(b"HJ"), Some("合江省"));
    }

    #[test]
    fn institution_codes_are_three_or_four_uppercase_and_unique() {
        assert_eq!(INSTITUTION_CODE_INFOS.len(), 104);
        for info in INSTITUTION_CODE_INFOS {
            let text = info.institution_code_text;
            assert!(text.len() == 3 || text.len() == 4);
            assert!(text.chars().all(|ch| ch.is_ascii_uppercase()));
            assert_eq!(institution_code_from_str(text), Some(info.institution_code));
            assert_eq!(
                institution_code_from_str(info.institution_code_label),
                Some(info.institution_code)
            );
        }
        for (i, institution_code) in ALL_CODES.iter().enumerate() {
            for other_institution_code in ALL_CODES.iter().skip(i + 1) {
                assert_ne!(
                    institution_code, other_institution_code,
                    "institution duplicate"
                );
            }
        }
    }

    #[test]
    fn classification_spot_check() {
        assert_eq!(institution_code_text(&NRC), Some("NRC"));
        assert_eq!(institution_code_label(&NRC), Some("国家储委会"));
        assert!(is_fixed_governance_code(&NRC));
        assert!(!is_registered_multisig_code(&NRC));
        assert!(is_public_legal_code(&NRC));
        assert!(is_fixed_governance_code(&FRG));
        assert!(!is_registered_multisig_code(&FRG));
        assert!(!is_institution_code(&FRG));
        assert!(is_fixed_governance_code(&NJD));
        assert!(!is_registered_multisig_code(&NJD));
        assert!(is_public_legal_code(&NJD));
        assert_eq!(
            institution_code_label(&code_bytes("FDA")),
            Some("食品药品监管局")
        );
        assert_eq!(
            institution_code_label(&code_bytes("NGB")),
            Some("国民警卫局")
        );
        assert_eq!(admin_level(&code_bytes("ARM")), Some(AdminLevel::National));
        assert_eq!(admin_level(&code_bytes("NGC")), Some(AdminLevel::National));

        assert!(is_personal_code(&PMUL));
        assert!(is_registered_multisig_code(&PMUL));
        assert!(!is_institution_code(&PMUL));

        assert!(is_institution_code(b"CGOV"));
        assert!(is_institution_code(b"SFLP"));
        assert!(is_unincorporated_code(b"UNIN"));
        assert!(is_person_code(b"CTZN"));
        assert!(!is_registered_multisig_code(b"CTZN"));
    }

    #[test]
    fn profit_policy_spot_check() {
        assert_eq!(profit_policy(b"SFGQ"), Some(ProfitPolicy::Profit));
        assert_eq!(profit_policy(b"SFGY"), Some(ProfitPolicy::NonProfit));
        assert_eq!(profit_policy(b"SFAS"), Some(ProfitPolicy::Variable));
        assert_eq!(profit_policy(b"SMTP"), Some(ProfitPolicy::Variable));
        assert_eq!(profit_policy(b"UNIN"), Some(ProfitPolicy::InheritParent));
    }

    #[test]
    fn fixed_governance_thresholds_match_constants() {
        assert_eq!(fixed_governance_pass_threshold(&NRC), Some(13));
        assert_eq!(fixed_governance_pass_threshold(&PRC), Some(6));
        assert_eq!(fixed_governance_pass_threshold(&PRB), Some(6));
        assert_eq!(fixed_governance_pass_threshold(&FRG), Some(3));
        assert_eq!(fixed_governance_pass_threshold(&NJD), Some(8));
        assert_eq!(fixed_governance_pass_threshold(&PMUL), None);
        assert_eq!(fixed_governance_pass_threshold(b"CGOV"), None);
    }

    #[test]
    fn code_bytes_pads_three_char() {
        assert_eq!(code_bytes("NRC"), *b"NRC\0");
        assert_eq!(code_bytes("CGOV"), *b"CGOV");
    }
}

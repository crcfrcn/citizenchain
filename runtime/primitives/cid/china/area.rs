//! 行政区常量表(创世直铸真源,ADR-031 卡3)。
//!
//! 数据由 `citizenchain/scripts/gen_area_data.py` 从 china.sqlite(行政区唯一真源 ADR-021)生成,
//! 编成紧凑二进制 `area_data.bin`,本模块 no_std 零拷贝解析,供 genesis 直铸
//! 「行政区 × 机构码模板」全部市行政区/镇行政区公权机构。
//!
//! 二进制格式(小端 u16):
//!   u16 省数
//!   每省: [2]省码 u8 名长 名; u16 市数
//!     每市: [3]市码 u8 名长 名; u16 镇数
//!       每镇: [3]镇码 u8 名长 名

/// china.sqlite 派生的行政区快照(重生走 `citizenchain/scripts/gen_area_data.py`)。
pub const AREA_DATA: &[u8] = include_bytes!("area_data.bin");

/// 零拷贝游标:按格式顺序读省/市/镇。所有 &str 借用自 `AREA_DATA`。
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn u16(&mut self) -> u16 {
        let v = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        v
    }

    fn fixed(&mut self, n: usize) -> &'a str {
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        // 数据由生成器保证 ascii,构建期解析失败即 panic(纵深防御)。
        core::str::from_utf8(s).expect("area code ascii")
    }

    fn var(&mut self) -> &'a str {
        let len = self.data[self.pos] as usize;
        self.pos += 1;
        let s = &self.data[self.pos..self.pos + len];
        self.pos += len;
        core::str::from_utf8(s).expect("area name utf-8")
    }
}

/// 一个市行政区(用于市行政区机构直铸与作为省行政区/国家机构的落点)。
pub struct CityArea<'a> {
    pub province_code: &'a str,
    pub province_name: &'a str,
    pub city_code: &'a str,
    pub city_name: &'a str,
}

/// 一个镇行政区(用于镇行政区机构直铸)。
pub struct TownArea<'a> {
    pub province_code: &'a str,
    pub province_name: &'a str,
    pub city_code: &'a str,
    pub city_name: &'a str,
    pub town_code: &'a str,
    pub town_name: &'a str,
}

/// 遍历项:省(含归属主市)/市/镇。单回调避免多闭包同时可变借用。
pub enum AreaItem<'a> {
    /// 省行政区部门落点:该省主市(市码 "001" 或首个市)。
    Province {
        province_code: &'a str,
        province_name: &'a str,
        home_city_code: &'a str,
        home_city_name: &'a str,
    },
    City(CityArea<'a>),
    Town(TownArea<'a>),
}

/// 单回调遍历省/市/镇。每省的 `City`/`Town` 先于该省 `Province` 项发出;
/// 遍历顺序与 `AREA_DATA` 字节序一致,确定性;genesis 与消费方共享本函数。
pub fn for_each_area<F>(mut f: F)
where
    F: FnMut(AreaItem),
{
    let mut cur = Cursor::new(AREA_DATA);
    let province_count = cur.u16();
    for _ in 0..province_count {
        let province_code = cur.fixed(2);
        let province_name = cur.var();
        let city_count = cur.u16();
        let mut home_city: Option<(&str, &str)> = None;
        for _ in 0..city_count {
            let city_code = cur.fixed(3);
            let city_name = cur.var();
            // 省行政区部门落点=市码 "001",否则首个市(与 onchina 生成一致):
            // 首个市先占位,遇到 "001" 覆盖。
            if home_city.is_none() || city_code == "001" {
                home_city = Some((city_code, city_name));
            }
            f(AreaItem::City(CityArea {
                province_code,
                province_name,
                city_code,
                city_name,
            }));
            let town_count = cur.u16();
            for _ in 0..town_count {
                let town_code = cur.fixed(3);
                let town_name = cur.var();
                f(AreaItem::Town(TownArea {
                    province_code,
                    province_name,
                    city_code,
                    city_name,
                    town_code,
                    town_name,
                }));
            }
        }
        if let Some((home_code, home_name)) = home_city {
            f(AreaItem::Province {
                province_code,
                province_name,
                home_city_code: home_code,
                home_city_name: home_name,
            });
        }
    }
}

/// 省数 / 市数 / 镇数(创世数量断言用)。
pub fn area_counts() -> (u32, u32, u32) {
    let mut provinces = 0u32;
    let mut cities = 0u32;
    let mut towns = 0u32;
    for_each_area(|item| match item {
        AreaItem::Province { .. } => provinces += 1,
        AreaItem::City(_) => cities += 1,
        AreaItem::Town(_) => towns += 1,
    });
    (provinces, cities, towns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_counts_match_china_sqlite_snapshot() {
        let (p, c, t) = area_counts();
        assert_eq!(p, 43, "省数");
        assert_eq!(c, 2872, "市数");
        assert_eq!(t, 39087, "镇数");
    }

    #[test]
    fn first_city_parses() {
        let mut first_city: Option<(alloc::string::String, alloc::string::String)> = None;
        for_each_area(|item| {
            if let AreaItem::City(city) = item {
                if first_city.is_none() {
                    first_city = Some((
                        alloc::string::ToString::to_string(city.province_code),
                        alloc::string::ToString::to_string(city.city_name),
                    ));
                }
            }
        });
        let (pc, name) = first_city.expect("has city");
        assert_eq!(pc.len(), 2);
        assert!(!name.is_empty());
    }
}

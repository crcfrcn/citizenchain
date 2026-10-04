//! 法律案 HTTP DTO + 转换为链编码器入参。
//!
//! 前端提交的法律案请求体 = 章节条款 + 标量(tier/scope_code/vote_type/标题/生效时间)。
//! **houses / executive / legislature 由后端按宪法路由 + 链上账户解析,不收前端**(防越权伪造表决院)。
//! 字段命名对齐链端 `legislation-yuan` 与 CitizenApp `law_models`。
//!

use super::chain_propose::{ArticleArg, ChapterArg, ClauseArg, SectionArg};
use serde::{Deserialize, Serialize};

/// 立法动作(对应 propose_enact/amend/repeal_law 三个 extrinsic)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LawActionInput {
    /// 立法(新法)。
    Enact,
    /// 修法(针对既有 law_id 提交新版本全文)。
    Amend,
    /// 废法(废止既有 law_id)。
    Repeal,
}

/// 款(最末层正文)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LawClause {
    pub number: u32,
    pub text: String,
    pub text_en: Option<String>,
}

/// 条(目录 + 正文 + 款列表)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LawArticle {
    pub number: u32,
    pub title: String,
    pub title_en: Option<String>,
    pub body: String,
    pub body_en: Option<String>,
    pub clauses: Vec<LawClause>,
}

/// 节(目录 + 条列表)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LawSection {
    pub number: u32,
    pub title: String,
    pub title_en: Option<String>,
    pub articles: Vec<LawArticle>,
}

/// 章(目录 + 节列表)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LawChapter {
    pub number: u32,
    pub title: String,
    pub title_en: Option<String>,
    pub sections: Vec<LawSection>,
}

/// 发起法律案请求体(houses/executive/legislature 后端解析,不收前端)。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposeLawInput {
    /// 立法动作。
    pub law_action: LawActionInput,
    /// 发起账户在当前机构内实际任职的岗位码。
    pub proposer_role_code: String,
    /// 层级(0 宪法 / 1 国家 / 2 省 / 3 市,对齐链 Tier::as_u8)。
    pub tier: u8,
    /// 层级行政区码(0 = 全国)。
    pub scope_code: u32,
    /// 表决类型(0 常规 / 1 常规教育 / 2 重要 / 3 重要教育 / 4 特别)。
    pub vote_type: u8,
    /// 法律标题(中文)。
    pub title: String,
    /// 法律标题(英文;宪法必填,普通法可空)。
    pub title_en: Option<String>,
    /// 正文:章>节>条>款(立法/修法携带;废法为空)。
    pub chapters: Vec<LawChapter>,
    /// 生效时间戳(毫秒,立法/修法)。
    pub effective_at: u64,
    /// 修法/废法目标法律 ID;立法(Enact)为 None。
    pub law_id: Option<u64>,
}

/// `Option<String>` → `Option<Vec<u8>>`(UTF-8 字节)。
fn opt_bytes(value: &Option<String>) -> Option<Vec<u8>> {
    value.as_ref().map(|s| s.clone().into_bytes())
}

/// 章节条款 DTO → 链编码器入参(String→Vec<u8>,逐层下沉)。
pub fn to_chapter_args(chapters: &[LawChapter]) -> Vec<ChapterArg> {
    chapters
        .iter()
        .map(|chapter| ChapterArg {
            number: chapter.number,
            title: chapter.title.clone().into_bytes(),
            title_en: opt_bytes(&chapter.title_en),
            sections: chapter
                .sections
                .iter()
                .map(|section| SectionArg {
                    number: section.number,
                    title: section.title.clone().into_bytes(),
                    title_en: opt_bytes(&section.title_en),
                    articles: section
                        .articles
                        .iter()
                        .map(|article| ArticleArg {
                            number: article.number,
                            title: article.title.clone().into_bytes(),
                            title_en: opt_bytes(&article.title_en),
                            body: article.body.clone().into_bytes(),
                            body_en: opt_bytes(&article.body_en),
                            clauses: article
                                .clauses
                                .iter()
                                .map(|clause| ClauseArg {
                                    number: clause.number,
                                    text: clause.text.clone().into_bytes(),
                                    text_en: opt_bytes(&clause.text_en),
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect()
}

// ──────────────── 读模型 DTO(链上 Law/LawVersion → 前端展示)────────────────

/// 立法代表机构引用；机构身份只保存 CID。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HouseRef {
    /// 机构唯一 CID。
    pub cid_number: String,
}

/// 法律只读视图(Law 主体 + 办理端展示版本全文,供操作端列表/详情与大屏展示)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LawView {
    pub law_id: u64,
    pub version: u32,
    /// 链上版本标签中文名;无标签时前端显示 `v{version}`。
    pub version_title: Option<String>,
    /// 链上版本标签英文名;英文展示无标签时前端显示 `v{version}`。
    pub version_title_en: Option<String>,
    /// 当前已生效版本;新法通过但未到生效时间时为空。
    pub effective_version: Option<u32>,
    /// 已写入链上的最新版本。
    pub latest_version: u32,
    /// 已通过但未到生效时间的版本。
    pub pending_version: Option<u32>,
    /// 层级(0 宪法 / 1 国家 / 2 省 / 3 市)。
    pub tier: u8,
    pub scope_code: u32,
    /// 法律状态(0 待生效 / 1 生效 / 2 废止)。
    pub status: u8,
    /// 表决类型(0 常规 / 1 常规教育 / 2 重要 / 3 重要教育 / 4 特别)。
    pub vote_type: u8,
    pub title: String,
    pub title_en: Option<String>,
    /// 正文哈希(0x hex)。
    pub content_hash: String,
    pub proposal_id: u64,
    pub published_at: u64,
    pub effective_at: u64,
    /// 表决院序列(机构 CID)。
    pub houses: Vec<HouseRef>,
    /// 宪法不可修改条款号;普通法律为空。
    pub immutable_article_numbers: Vec<u32>,
    /// 正文:章>节>条>款。
    pub chapters: Vec<LawChapter>,
}

/// 4 字节机构码 → 去尾 `\0` 的可读字符串。
pub fn institution_code_text(code: &[u8; 4]) -> String {
    let end = code.iter().position(|&b| b == 0).unwrap_or(code.len());
    String::from_utf8_lossy(&code[..end]).into_owned()
}

/// CID 原始字节 → 代表机构读模型。
pub fn house_ref(cid_number: &[u8]) -> HouseRef {
    HouseRef {
        cid_number: String::from_utf8_lossy(cid_number).into_owned(),
    }
}

/// `Option<Vec<u8>>` → `Option<String>`(非法 UTF-8 以 lossy 兜底,仅展示用)。
fn opt_string(value: &Option<Vec<u8>>) -> Option<String> {
    value
        .as_ref()
        .map(|b| String::from_utf8_lossy(b).into_owned())
}

/// 链编码器章节(字节)→ 显示用章节(String);`to_chapter_args` 的逆向。
pub fn to_law_chapters(chapters: &[ChapterArg]) -> Vec<LawChapter> {
    chapters
        .iter()
        .map(|chapter| LawChapter {
            number: chapter.number,
            title: String::from_utf8_lossy(&chapter.title).into_owned(),
            title_en: opt_string(&chapter.title_en),
            sections: chapter
                .sections
                .iter()
                .map(|section| LawSection {
                    number: section.number,
                    title: String::from_utf8_lossy(&section.title).into_owned(),
                    title_en: opt_string(&section.title_en),
                    articles: section
                        .articles
                        .iter()
                        .map(|article| LawArticle {
                            number: article.number,
                            title: String::from_utf8_lossy(&article.title).into_owned(),
                            title_en: opt_string(&article.title_en),
                            body: String::from_utf8_lossy(&article.body).into_owned(),
                            body_en: opt_string(&article.body_en),
                            clauses: article
                                .clauses
                                .iter()
                                .map(|clause| LawClause {
                                    number: clause.number,
                                    text: String::from_utf8_lossy(&clause.text).into_owned(),
                                    text_en: opt_string(&clause.text_en),
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect()
}

#[cfg(test)]
// 法律模型夹具必须通过固定校验，断言式解包仅限测试。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn proposal_request_deserializes_camel_case() {
        let json = r#"{
            "lawAction":"enact","tier":1,"scopeCode":0,"voteType":2,
            "proposerRoleCode":"REPRESENTATIVE",
            "title":"道路交通安全法","titleEn":"Road Traffic Safety Law",
            "chapters":[{"number":1,"title":"总则","sections":[]}],
            "effectiveAt":1000,"lawId":null
        }"#;
        let input: ProposeLawInput = serde_json::from_str(json).expect("parse");
        assert_eq!(input.law_action, LawActionInput::Enact);
        assert_eq!(input.tier, 1);
        assert_eq!(input.chapters.len(), 1);
        assert_eq!(input.chapters[0].title, "总则");
    }

    #[test]
    fn chapters_convert_to_encoder_args_preserving_text() {
        let chapters = vec![LawChapter {
            number: 1,
            title: "总则".to_string(),
            title_en: Some("General".to_string()),
            sections: vec![LawSection {
                number: 1,
                title: "定义".to_string(),
                title_en: None,
                articles: vec![LawArticle {
                    number: 1,
                    title: "第一条".to_string(),
                    title_en: None,
                    body: "正文".to_string(),
                    body_en: None,
                    clauses: vec![LawClause {
                        number: 1,
                        text: "第一款".to_string(),
                        text_en: None,
                    }],
                }],
            }],
        }];
        let args = to_chapter_args(&chapters);
        assert_eq!(args.len(), 1);
        assert_eq!(args[0].title, "总则".as_bytes());
        assert_eq!(args[0].title_en.as_deref(), Some("General".as_bytes()));
        assert_eq!(
            args[0].sections[0].articles[0].clauses[0].text,
            "第一款".as_bytes()
        );
        assert!(args[0].sections[0].title_en.is_none());
    }

    #[test]
    fn chapter_args_round_trip_back_to_display_chapters() {
        // to_chapter_args → to_law_chapters 往返:展示文本与层级结构无损还原。
        let original = vec![LawChapter {
            number: 1,
            title: "总则".to_string(),
            title_en: Some("General".to_string()),
            sections: vec![LawSection {
                number: 1,
                title: "定义".to_string(),
                title_en: None,
                articles: vec![LawArticle {
                    number: 1,
                    title: "第一条".to_string(),
                    title_en: None,
                    body: "正文".to_string(),
                    body_en: None,
                    clauses: vec![LawClause {
                        number: 1,
                        text: "第一款".to_string(),
                        text_en: None,
                    }],
                }],
            }],
        }];
        let restored = to_law_chapters(&to_chapter_args(&original));
        assert_eq!(restored[0].title, "总则");
        assert_eq!(restored[0].title_en.as_deref(), Some("General"));
        assert_eq!(restored[0].sections[0].articles[0].body, "正文");
        assert_eq!(
            restored[0].sections[0].articles[0].clauses[0].text,
            "第一款"
        );
    }

    #[test]
    fn institution_code_text_trims_trailing_nul() {
        assert_eq!(institution_code_text(b"NRP\0"), "NRP");
        assert_eq!(institution_code_text(b"CLEG"), "CLEG");
        let house = house_ref(b"LN001-NSN0G-000000001-2026");
        assert_eq!(house.cid_number, "LN001-NSN0G-000000001-2026");
    }
}

//! 立法院 `propose_enact/amend/repeal_law` 裸 SCALE call-data 编码器(onchina 侧唯一真源)。
//!
//! 复用 `core::institution_call` 与 `core::chain_submit` 的「构造裸 call data → CitizenWallet
//! 一次签名并显示响应二维码 → OnChina 回扫后统一提交」通道。
//!
//! **铁律**:参数顺序与 SCALE 类型必须与链端 `legislation-yuan`(pallet idx 25)逐字节一致:
//! - `tier` / `vote_type` 是单字节枚举序号(Tier:0宪法/1国家/2省/3市;VoteType:0常规/1常规教育/2重要/3重要教育/4特别);
//! - `houses` = `Vec<CidNumber>`,带 `Compact<u32>` 数量前缀,每项是 CID 字节向量;
//! - `actor_cid_number` / `executive_cid_number` = 机构唯一 CID;
//! - `proposer_role_code` = 发起账户在 actor 机构内实际任职的岗位码;
//! - `legislature_cid_number` = `Option<CidNumber>`;
//! - `title` / `title_en` = `Vec<u8>` / `Option<Vec<u8>>`(`Compact<u32>` 长度前缀);
//! - `chapters` = 章>节>条>款 嵌套(链端 `BoundedVec` 与 `Vec` 的 SCALE 同布局,由 `ChapterArg` 派生 `Encode`);
//! - `scope_code` = u32 小端;`effective_at` = 生效时间戳毫秒(u64 小端);`law_id` = u64 小端。
//!
//! `tests` 用链端真实 `legislation_yuan::{Tier,VoteType}` 与 codec `.encode()` 逐字节交叉校验,杜绝静默漂移。
//!
//! 随 Phase 1B 接入,届时移除本 allow。

use crate::core::institution_call::{chain_action_code, ChainCall};
use codec::{Compact, Decode, Encode};

/// LegislationYuan pallet 在 construct_runtime 的索引。
pub const LEGISLATION_YUAN_PALLET_INDEX: u8 = 25;
/// `propose_enact_law` call index。
pub const PROPOSE_ENACT_LAW_CALL_INDEX: u8 = 0;
/// `propose_amend_law` call index。
pub const PROPOSE_AMEND_LAW_CALL_INDEX: u8 = 1;
/// `propose_repeal_law` call index。
pub const PROPOSE_REPEAL_LAW_CALL_INDEX: u8 = 2;

// 章>节>条>款 SCALE 镜像:`Vec` 与链端 `BoundedVec` SCALE 同布局,字段顺序锁死链端
// `legislation-yuan` 的 `Chapter/Section/Article/Clause`(lib.rs:75/97/125/149)。
/// 款(最末层正文)。
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ClauseArg {
    pub number: u32,
    pub text: Vec<u8>,
    pub text_en: Option<Vec<u8>>,
}
/// 条(目录 + 正文 + 款列表)。
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ArticleArg {
    pub number: u32,
    pub title: Vec<u8>,
    pub title_en: Option<Vec<u8>>,
    pub body: Vec<u8>,
    pub body_en: Option<Vec<u8>>,
    pub clauses: Vec<ClauseArg>,
}
/// 节(目录 + 条列表)。
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SectionArg {
    pub number: u32,
    pub title: Vec<u8>,
    pub title_en: Option<Vec<u8>>,
    pub articles: Vec<ArticleArg>,
}
/// 章(目录 + 节列表)。
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ChapterArg {
    pub number: u32,
    pub title: Vec<u8>,
    pub title_en: Option<Vec<u8>>,
    pub sections: Vec<SectionArg>,
}

/// `Vec<u8>`:`Compact<u32>` 长度前缀 + 原始字节。
fn encode_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend(Compact(bytes.len() as u32).encode());
    out.extend_from_slice(bytes);
}

/// `Option<Vec<u8>>`:0x00 None / 0x01 + `Vec<u8>`。
fn encode_opt_bytes(out: &mut Vec<u8>, bytes: Option<&[u8]>) {
    match bytes {
        None => out.push(0x00),
        Some(b) => {
            out.push(0x01);
            encode_bytes(out, b);
        }
    }
}

/// `houses: Vec<CidNumber>`:`Compact<u32>` 数量前缀 + 各 CID 字节向量。
fn encode_houses(out: &mut Vec<u8>, houses: &[Vec<u8>]) {
    out.extend(Compact(houses.len() as u32).encode());
    for cid_number in houses {
        encode_bytes(out, cid_number);
    }
}

/// `chapters: Vec<Chapter>`:`Compact<u32>` 数量前缀 + 各 Chapter(派生 `Encode`)。
fn encode_chapters(out: &mut Vec<u8>, chapters: &[ChapterArg]) {
    out.extend(Compact(chapters.len() as u32).encode());
    for chapter in chapters {
        out.extend(chapter.encode());
    }
}

/// 立法(新法):pallet 25 call 0。
#[allow(clippy::too_many_arguments)]
pub fn encode_propose_enact_law(
    tier: u8,
    scope_code: u32,
    houses: &[Vec<u8>],
    actor_cid_number: &[u8],
    proposer_role_code: &[u8],
    executive_cid_number: &[u8],
    legislature_cid_number: Option<&[u8]>,
    vote_type: u8,
    title: &[u8],
    title_en: Option<&[u8]>,
    chapters: &[ChapterArg],
    effective_at: u64,
) -> ChainCall {
    let mut out = vec![LEGISLATION_YUAN_PALLET_INDEX, PROPOSE_ENACT_LAW_CALL_INDEX];
    out.push(tier);
    out.extend(scope_code.to_le_bytes());
    encode_houses(&mut out, houses);
    encode_bytes(&mut out, actor_cid_number);
    encode_bytes(&mut out, proposer_role_code);
    encode_bytes(&mut out, executive_cid_number);
    encode_opt_bytes(&mut out, legislature_cid_number);
    out.push(vote_type);
    encode_bytes(&mut out, title);
    encode_opt_bytes(&mut out, title_en);
    encode_chapters(&mut out, chapters);
    out.extend(effective_at.to_le_bytes());
    ChainCall {
        action: chain_action_code(LEGISLATION_YUAN_PALLET_INDEX, PROPOSE_ENACT_LAW_CALL_INDEX),
        call_data: out,
    }
}

/// 修法:pallet 25 call 1(`law_id` 取代 `tier`/`scope_code`)。
#[allow(clippy::too_many_arguments)]
pub fn encode_propose_amend_law(
    law_id: u64,
    actor_cid_number: &[u8],
    proposer_role_code: &[u8],
    executive_cid_number: &[u8],
    legislature_cid_number: Option<&[u8]>,
    vote_type: u8,
    title: &[u8],
    title_en: Option<&[u8]>,
    chapters: &[ChapterArg],
    effective_at: u64,
) -> ChainCall {
    let mut out = vec![LEGISLATION_YUAN_PALLET_INDEX, PROPOSE_AMEND_LAW_CALL_INDEX];
    out.extend(law_id.to_le_bytes());
    encode_bytes(&mut out, actor_cid_number);
    encode_bytes(&mut out, proposer_role_code);
    encode_bytes(&mut out, executive_cid_number);
    encode_opt_bytes(&mut out, legislature_cid_number);
    out.push(vote_type);
    encode_bytes(&mut out, title);
    encode_opt_bytes(&mut out, title_en);
    encode_chapters(&mut out, chapters);
    out.extend(effective_at.to_le_bytes());
    ChainCall {
        action: chain_action_code(LEGISLATION_YUAN_PALLET_INDEX, PROPOSE_AMEND_LAW_CALL_INDEX),
        call_data: out,
    }
}

/// 废法:pallet 25 call 2(无 `title`/`chapters`/`effective_at`)。
pub fn encode_propose_repeal_law(
    law_id: u64,
    actor_cid_number: &[u8],
    proposer_role_code: &[u8],
    executive_cid_number: &[u8],
    legislature_cid_number: Option<&[u8]>,
    vote_type: u8,
) -> ChainCall {
    let mut out = vec![LEGISLATION_YUAN_PALLET_INDEX, PROPOSE_REPEAL_LAW_CALL_INDEX];
    out.extend(law_id.to_le_bytes());
    encode_bytes(&mut out, actor_cid_number);
    encode_bytes(&mut out, proposer_role_code);
    encode_bytes(&mut out, executive_cid_number);
    encode_opt_bytes(&mut out, legislature_cid_number);
    out.push(vote_type);
    ChainCall {
        action: chain_action_code(LEGISLATION_YUAN_PALLET_INDEX, PROPOSE_REPEAL_LAW_CALL_INDEX),
        call_data: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use legislation_yuan::{Tier, VoteType};

    /// 本编码器的 `tier`/`vote_type` 单字节序号必须与链端真实枚举 `.encode()` 逐字节一致。
    #[test]
    fn tier_and_vote_type_bytes_match_runtime_enums() {
        for (byte, real) in [
            (0u8, Tier::Constitution),
            (1, Tier::National),
            (2, Tier::Provincial),
            (3, Tier::Municipal),
        ] {
            assert_eq!(vec![byte], real.encode(), "Tier 序号漂移");
        }
        for (byte, real) in [
            (0u8, VoteType::Regular),
            (1, VoteType::RegularEducation),
            (2, VoteType::Major),
            (3, VoteType::MajorEducation),
            (4, VoteType::Special),
        ] {
            assert_eq!(vec![byte], real.encode(), "VoteType 序号漂移");
        }
    }

    fn cid(suffix: &str) -> Vec<u8> {
        format!("LN001-{suffix}0G-000000001-2026").into_bytes()
    }

    fn sample_chapters() -> Vec<ChapterArg> {
        vec![ChapterArg {
            number: 1,
            title: "总则".as_bytes().to_vec(),
            title_en: Some("General".as_bytes().to_vec()),
            sections: vec![SectionArg {
                number: 1,
                title: "定义".as_bytes().to_vec(),
                title_en: None,
                articles: vec![ArticleArg {
                    number: 1,
                    title: "第一条".as_bytes().to_vec(),
                    title_en: None,
                    body: "正文".as_bytes().to_vec(),
                    body_en: None,
                    clauses: vec![ClauseArg {
                        number: 1,
                        text: "第一款".as_bytes().to_vec(),
                        text_en: None,
                    }],
                }],
            }],
        }]
    }

    /// 完整 `propose_enact_law` 编码:用链端真实 `Tier`/`VoteType` + codec 逐参数拼 golden,逐字节比对。
    #[test]
    fn enact_law_call_matches_codec_golden_and_prefix() {
        let houses = vec![cid("NRP"), cid("NSN")];
        let actor_cid_number = cid("NRP");
        let proposer_role_code = b"REPRESENTATIVE";
        let executive_cid_number = cid("PRS");
        let legislature_cid_number = cid("NLG");
        let chapters = sample_chapters();
        let title = "道路交通安全法".as_bytes();
        let title_en = "Road Traffic Safety Law".as_bytes();

        let chain = encode_propose_enact_law(
            1, // tier=National
            0, // scope_code 全国
            &houses,
            &actor_cid_number,
            proposer_role_code,
            &executive_cid_number,
            Some(&legislature_cid_number),
            2, // vote_type=Major
            title,
            Some(title_en),
            &chapters,
            1000,
        );

        // 前缀 [25,0] + QR 动作码 0x1900 = (25<<8)|0。
        assert_eq!(&chain.call_data[..2], &[25, 0]);
        assert_eq!(chain.action, 0x1900);

        let mut golden = Vec::new();
        golden.extend(Tier::National.encode());
        golden.extend(0u32.encode());
        golden.extend(houses.encode());
        golden.extend(actor_cid_number.encode());
        golden.extend(proposer_role_code.to_vec().encode());
        golden.extend(executive_cid_number.encode());
        golden.extend(Some(legislature_cid_number).encode());
        golden.extend(VoteType::Major.encode());
        golden.extend(title.to_vec().encode());
        golden.extend(Some(title_en.to_vec()).encode());
        golden.extend(chapters.encode());
        golden.extend(1000u64.encode());

        assert_eq!(
            &chain.call_data[2..],
            &golden[..],
            "enact call SCALE 与链端类型漂移"
        );
    }

    /// 修法/废法前缀、`law_id`(u64 小端)与废法尾部(无 chapters,末字节=vote_type)。
    #[test]
    fn amend_and_repeal_prefix_law_id_and_repeal_tail() {
        let actor_cid_number = cid("PRP");
        let proposer_role_code = b"REPRESENTATIVE";
        let executive_cid_number = cid("PGV");
        let legislature_cid_number = cid("PLG");

        let amend = encode_propose_amend_law(
            7,
            &actor_cid_number,
            proposer_role_code,
            &executive_cid_number,
            Some(&legislature_cid_number),
            0,
            b"x",
            None,
            &sample_chapters(),
            50,
        );
        assert_eq!(&amend.call_data[..2], &[25, 1]);
        assert_eq!(amend.action, 0x1901);
        assert_eq!(&amend.call_data[2..10], &7u64.to_le_bytes());

        let repeal = encode_propose_repeal_law(
            7,
            &actor_cid_number,
            proposer_role_code,
            &executive_cid_number,
            None,
            4,
        );
        assert_eq!(&repeal.call_data[..2], &[25, 2]);
        assert_eq!(&repeal.call_data[2..10], &7u64.to_le_bytes());
        // 废法尾 = actor CID + 发起岗位码 + executive CID + legislature(None) + vote_type。
        let mut golden_tail = Vec::new();
        golden_tail.extend(actor_cid_number.encode());
        golden_tail.extend(proposer_role_code.to_vec().encode());
        golden_tail.extend(executive_cid_number.encode());
        golden_tail.extend(Option::<Vec<u8>>::None.encode());
        golden_tail.push(4u8);
        assert_eq!(&repeal.call_data[10..], &golden_tail[..]);
    }
}

//! 立法院 `propose_enact/amend/repeal_law` 裸 SCALE call-data 编码器(onchina 侧唯一真源)。
//!
//! 复用 `onchina::institutions_call` 与 `onchina::chain_submit` 的「构造裸 call data → CitizenWallet
//! 一次签名并显示响应二维码 → OnChina 回扫后统一提交」通道。
//!
//! **铁律**:参数顺序与 SCALE 类型必须与链端 `legislation`(pallet idx 25)逐字节一致:
//! - `tier` / `vote_type` 是单字节枚举序号(Tier:0宪法/1国家/2省/3市;VoteType:0常规/1常规教育/2重要/3重要教育/4特别);
//! - `houses` = `Vec<CidNumber>`,带 `Compact<u32>` 数量前缀,每项是 CID 字节向量;
//! - `actor_cid_number` / `executive_cid_number` = 机构唯一 CID;
//! - `proposer_role_code` = 发起账户在 actor 机构内实际任职的岗位码;
//! - `legislature_cid_number` = `Option<CidNumber>`;
//! - `title` / `title_en` = `Vec<u8>` / `Option<Vec<u8>>`(`Compact<u32>` 长度前缀);
//! - `chapters` = 章>节>条>款 嵌套(链端 `BoundedVec` 与 `Vec` 的 SCALE 同布局,由 `ChapterArg` 派生 `Encode`);
//! - `region` = 原两字母省码 + u16 小端市号(固定四字节);`effective_at` = 生效时间戳毫秒(u64 小端);`law_id` = u64 小端。
//!
//! `tests` 用链端真实 `legislation::{Tier,VoteType}` 与 codec `.encode()` 逐字节交叉校验,杜绝静默漂移。
//!
//! prepare 由 action.rs 调用，统一提交由平台 transactions 负责。

use codec::{Compact, Decode, Encode};
use onchina::institutions_call::{chain_action_code, ChainCall};
use primitives::genesis::LawRegion;

/// LegislationYuan pallet 在 construct_runtime 的索引。
pub const LEGISLATION_YUAN_PALLET_INDEX: u8 = 25;
/// `propose_enact_law` call index。
pub const PROPOSE_ENACT_LAW_CALL_INDEX: u8 = 0;
/// `propose_amend_law` call index。
pub const PROPOSE_AMEND_LAW_CALL_INDEX: u8 = 1;
/// `propose_repeal_law` call index。
pub const PROPOSE_REPEAL_LAW_CALL_INDEX: u8 = 2;

// 章>节>条>款 SCALE 镜像:`Vec` 与链端 `BoundedVec` SCALE 同布局,字段顺序锁死链端
// `legislation` 的 `Chapter/Section/Article/Clause`(lib.rs:75/97/125/149)。
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
    region: LawRegion,
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
    out.extend(region.encode());
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

/// 修法:pallet 25 call 1(`law_id` 取代 `tier`/`region`)。
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
#[path = "../tests/laws_propose.rs"]
mod tests;

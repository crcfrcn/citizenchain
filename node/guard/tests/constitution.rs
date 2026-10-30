use super::*;

use crate::constitution::check_core_referendum_proof;
use crate::constitution::check_frozen_constitution_records;
use crate::constitution::check_guard_review_proof;
use crate::constitution::check_immutable_articles;
use crate::constitution::check_immutable_version;
use crate::constitution::check_imported_state_immutable;
use crate::constitution::check_imported_state_key_values;
use crate::constitution::check_version_key_range;
use crate::constitution::declared_constitution_versions;
use crate::constitution::decode_law_head;
use crate::constitution::needs_full_invariant_check;
use crate::constitution::render::effective_version_of_law;
use crate::constitution::render::render_constitution_html;
use crate::constitution::validate_version_identity_and_structure;
use crate::constitution::verify_manifest;
use crate::constitution::GuardError;
use crate::constitution::ImmutableReference;
use crate::constitution::MArticle;
use crate::constitution::MChapter;
use crate::constitution::MImmutableManifest;
use crate::constitution::MLawVersionHead;
use crate::constitution::MSection;
use crate::constitution::CONSTITUTION_LAW_ID;
use crate::constitution::GENESIS_CONSTITUTION_VERSION;
use crate::constitution::LAW_STATUS_EFFECTIVE;
use crate::constitution::LAW_STATUS_PENDING;
use crate::constitution::LAW_STATUS_REPEALED;
use crate::constitution::LAW_VOTE_TYPE_SPECIAL;
use crate::constitution::TIER_CONSTITUTION;
use primitives::count_const::IMMUTABLE_CONSTITUTION_ARTICLES;
use runtime::opaque::Block;
use sc_client_api::backend::Backend as _;
use std::collections::BTreeMap;

// ---- 测试夹具:构造一份 LawVersion 字节(可指定某条文内容)----
fn article_bytes(number: u32, body: &str) -> MArticle {
    MArticle {
        number,
        title: format!("第{number}条").into_bytes(),
        title_en: Some(format!("Article {number}").into_bytes()),
        body: body.as_bytes().to_vec(),
        body_en: Some("EN".as_bytes().to_vec()),
        clauses: Vec::new(),
    }
}

fn section_bytes(number: u32, articles: Vec<MArticle>) -> MSection {
    MSection {
        number,
        title: format!("第{number}节").into_bytes(),
        title_en: None,
        articles,
    }
}

fn chapter_bytes(number: u32, sections: Vec<MSection>) -> MChapter {
    MChapter {
        number,
        title: format!("第{number}章").into_bytes(),
        title_en: None,
        sections,
    }
}

fn version_head(chapters: Vec<MChapter>) -> MLawVersionHead {
    MLawVersionHead {
        law_id: CONSTITUTION_LAW_ID,
        version: GENESIS_CONSTITUTION_VERSION,
        title: b"constitution".to_vec(),
        title_en: None,
        content_hash: sp_crypto_hashing::blake2_256(&chapters.encode()),
        chapters,
        vote_type: LAW_VOTE_TYPE_SPECIAL,
    }
}

/// 构造 LawVersion(version, 条文, vote_type)的 SCALE 字节 + 哑尾(模拟链端完整编码)。
/// 单章夹具:全部条文置于第一章(核心章),便于测核心章档位背书。
fn law_version_scale_vt(version: u32, articles: Vec<MArticle>, vote_type: u8) -> Vec<u8> {
    let chapter = MChapter {
        number: 1,
        title: "第一章".as_bytes().to_vec(),
        title_en: Some("Chapter I".as_bytes().to_vec()),
        sections: vec![MSection {
            number: 1,
            title: "第一节".as_bytes().to_vec(),
            title_en: Some("Section 1".as_bytes().to_vec()),
            articles,
        }],
    };
    let chapters = vec![chapter];
    let content_hash = sp_crypto_hashing::blake2_256(&chapters.encode());
    let mut bytes = Vec::new();
    CONSTITUTION_LAW_ID.encode_to(&mut bytes); // law_id
    version.encode_to(&mut bytes); // version
    "公民宪法".as_bytes().to_vec().encode_to(&mut bytes); // title
    Option::<Vec<u8>>::None.encode_to(&mut bytes); // title_en
    chapters.encode_to(&mut bytes); // chapters
    content_hash.encode_to(&mut bytes); // content_hash
    vote_type.encode_to(&mut bytes); // vote_type
    0u64.encode_to(&mut bytes); // proposal_id
    0u64.encode_to(&mut bytes); // published_at
    0u64.encode_to(&mut bytes); // effective_at
    bytes
}

/// 默认版本编码:vote_type = 特别案(与创世宪法 `VoteType::Special` 一致)。
fn law_version_scale(version: u32, articles: Vec<MArticle>) -> Vec<u8> {
    law_version_scale_vt(version, articles, LAW_VOTE_TYPE_SPECIAL)
}

fn law_version_label_scale(title: &str, title_en: Option<&str>) -> Vec<u8> {
    let mut bytes = Vec::new();
    title.as_bytes().to_vec().encode_to(&mut bytes);
    title_en
        .map(|s| s.as_bytes().to_vec())
        .encode_to(&mut bytes);
    bytes
}

/// 构造 Law 的 SCALE 字节 + 哑尾。
/// Law(0) 字节:tier=Constitution、scope=0、给定 houses/显式版本指针/status。
fn law_scale_with_versions(
    effective_version: Option<u32>,
    latest_version: u32,
    pending_version: Option<u32>,
    status: u8,
    houses: Vec<Vec<u8>>,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    CONSTITUTION_LAW_ID.encode_to(&mut bytes); // law_id
    TIER_CONSTITUTION.encode_to(&mut bytes); // tier = Constitution(0)
    0u32.encode_to(&mut bytes); // scope_code = 0
    houses.encode_to(&mut bytes); // houses
    effective_version.encode_to(&mut bytes); // effective_version
    latest_version.encode_to(&mut bytes); // latest_version
    pending_version.encode_to(&mut bytes); // pending_version
    status.encode_to(&mut bytes); // status
    bytes
}

fn law_scale_full(latest_version: u32, status: u8, houses: Vec<Vec<u8>>) -> Vec<u8> {
    let (effective_version, pending_version) = if status == LAW_STATUS_PENDING {
        (
            (latest_version > 1).then_some(latest_version - 1),
            Some(latest_version),
        )
    } else {
        (Some(latest_version), None)
    };
    law_scale_with_versions(
        effective_version,
        latest_version,
        pending_version,
        status,
        houses,
    )
}

/// 默认合法 Law(0):Effective、空 houses。
fn law_scale(latest_version: u32) -> Vec<u8> {
    law_scale_full(latest_version, 1 /* Effective */, Vec::new())
}

fn laws_by_scope_entry(list: Vec<u64>) -> (Vec<u8>, Vec<u8>) {
    (
        crate::constitution::storage_key::laws_by_scope_constitution(),
        list.encode(),
    )
}

/// 一条核心修宪永久公投凭据:`ConstitutionAmendmentProof[version] = (eligible, yes, no)`。
fn amendment_proof_entry(version: u32, eligible: u64, yes: u64, no: u64) -> (Vec<u8>, Vec<u8>) {
    (
        crate::constitution::storage_key::constitution_amendment_proof(version),
        (eligible, yes, no).encode(),
    )
}

/// 一条修宪护宪终审凭据:`ConstitutionGuardVoteProof[version] = approve`。
fn guard_proof_entry(version: u32, approve: u32) -> (Vec<u8>, Vec<u8>) {
    (
        crate::constitution::storage_key::constitution_guard_vote_proof(version),
        approve.encode(),
    )
}

/// 创世 manifest 固定引用创世版本中的不可修改条款规范 SCALE 字节。
fn manifest_entry() -> (Vec<u8>, Vec<u8>) {
    let articles = genesis_articles();
    let article_numbers = IMMUTABLE_CONSTITUTION_ARTICLES.to_vec();
    let article_hashes = article_numbers
        .iter()
        .map(|number| {
            let article = articles
                .iter()
                .find(|article| article.number == *number)
                .expect("创世夹具应含全部不可修改条款");
            sp_crypto_hashing::blake2_256(&article.encode())
        })
        .collect();
    (
        crate::constitution::storage_key::manifest(),
        MImmutableManifest {
            article_numbers,
            article_hashes,
        }
        .encode(),
    )
}

/// 一份完整合法当前态:Laws[0] + LawVersions[0][version] + LawsByScope[宪法][0]=[0]。
fn valid_current_state(version: u32, articles: Vec<MArticle>) -> Vec<(Vec<u8>, Vec<u8>)> {
    valid_current_state_vt(version, articles, LAW_VOTE_TYPE_SPECIAL)
}

/// 同 `valid_current_state`,但显式指定生效版本记录的 `vote_type`(测核心章档位背书)。
/// 修宪版本(v>创世)自动挂一份通过口径(4/7)的护宪终审凭据,使其为一份**合法**修宪态;
/// 需测护宪凭据缺失/不合格的用例请手工构造(不经本 helper)。
fn valid_current_state_vt(
    version: u32,
    articles: Vec<MArticle>,
    vote_type: u8,
) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut entries = vec![
        (
            crate::constitution::storage_key::law(CONSTITUTION_LAW_ID),
            law_scale(version),
        ),
        (
            crate::constitution::storage_key::law_version(CONSTITUTION_LAW_ID, version),
            law_version_scale_vt(version, articles, vote_type),
        ),
        laws_by_scope_entry(vec![CONSTITUTION_LAW_ID]),
        manifest_entry(),
    ];
    if version > GENESIS_CONSTITUTION_VERSION {
        entries.push((
            crate::constitution::storage_key::law_version(
                CONSTITUTION_LAW_ID,
                GENESIS_CONSTITUTION_VERSION,
            ),
            law_version_scale(GENESIS_CONSTITUTION_VERSION, genesis_articles()),
        ));
        entries.push(guard_proof_entry(version, 4));
    }
    entries
}

/// 一份合法待生效态：创世版本仍生效，修订版本待生效。
fn valid_pending_state(pending_articles: Vec<MArticle>) -> Vec<(Vec<u8>, Vec<u8>)> {
    vec![
        (
            crate::constitution::storage_key::law(CONSTITUTION_LAW_ID),
            law_scale_with_versions(Some(1), 2, Some(2), LAW_STATUS_PENDING, Vec::new()),
        ),
        (
            crate::constitution::storage_key::law_version(CONSTITUTION_LAW_ID, 1),
            law_version_scale(1, genesis_articles()),
        ),
        (
            crate::constitution::storage_key::law_version(CONSTITUTION_LAW_ID, 2),
            law_version_scale(2, pending_articles),
        ),
        laws_by_scope_entry(vec![CONSTITUTION_LAW_ID]),
        manifest_entry(),
        // 待生效版本 v2 是一次修宪 → 挂通过口径的护宪终审凭据(第21条)。
        guard_proof_entry(2, 4),
    ]
}

/// 用一组 (key,value) 建一个 RAW 读取闭包。
fn reader(entries: Vec<(Vec<u8>, Vec<u8>)>) -> impl Fn(&[u8]) -> Option<Vec<u8>> {
    let map: BTreeMap<Vec<u8>, Vec<u8>> = entries.into_iter().collect();
    move |k: &[u8]| map.get(k).cloned()
}

/// 在完整状态夹具中替换一个 RAW key，避免测试误留同键双记录。
fn replace_entry(entries: &mut Vec<(Vec<u8>, Vec<u8>)>, key: Vec<u8>, value: Vec<u8>) {
    entries.retain(|(existing, _)| existing != &key);
    entries.push((key, value));
}

// 取全部不可修改条号 + 几条可变条文,组成一份"创世"状态。
fn genesis_articles() -> Vec<MArticle> {
    let mut arts: Vec<MArticle> = IMMUTABLE_CONSTITUTION_ARTICLES
        .iter()
        .map(|&n| article_bytes(n, &format!("不可修改条款 {n} 原文")))
        .collect();
    arts.push(article_bytes(5, "可变条款原文")); // 一条可变条
    arts
}

fn genesis_state() -> Vec<(Vec<u8>, Vec<u8>)> {
    valid_current_state(1, genesis_articles())
}

/// 不可修改条款原样 + 可变条改动的新版本条文(version=2 用)。
fn amended_articles(immutable_body: impl Fn(u32) -> String) -> Vec<MArticle> {
    let mut arts: Vec<MArticle> = IMMUTABLE_CONSTITUTION_ARTICLES
        .iter()
        .map(|&n| article_bytes(n, &immutable_body(n)))
        .collect();
    arts.push(article_bytes(5, "可变条款原文"));
    arts
}

fn immutable_intact(n: u32) -> String {
    format!("不可修改条款 {n} 原文")
}

#[test]
fn key_derivation_is_stable_and_distinct() {
    // 同输入稳定、不同输入相异、含正确前缀。
    assert_eq!(
        crate::constitution::storage_key::law(0),
        crate::constitution::storage_key::law(0)
    );
    assert_ne!(
        crate::constitution::storage_key::law(0),
        crate::constitution::storage_key::law(1)
    );
    assert_ne!(
        crate::constitution::storage_key::law_version(0, 1),
        crate::constitution::storage_key::law_version(0, 2)
    );
    assert_ne!(
        crate::constitution::storage_key::law_version(0, 1),
        crate::constitution::storage_key::law_version_label(0, 1)
    );
    assert!(crate::constitution::storage_key::law(0)
        .starts_with(&crate::constitution::storage_key::pallet_prefix()));
    assert!(crate::constitution::storage_key::law_version(0, 1)
        .starts_with(&crate::constitution::storage_key::pallet_prefix()));
    assert!(crate::constitution::storage_key::law_version_label(0, 1)
        .starts_with(&crate::constitution::storage_key::pallet_prefix()));
    assert_eq!(
        crate::constitution::storage_key::constitution_proof_version_from_key(
            &crate::constitution::storage_key::constitution_amendment_proof(7)
        ),
        Some(7)
    );
    assert_eq!(
        crate::constitution::storage_key::constitution_proof_version_from_key(
            &crate::constitution::storage_key::constitution_guard_vote_proof(8)
        ),
        Some(8)
    );

    let mut malformed_version = crate::constitution::storage_key::law_version(0, 9);
    let hash_offset = crate::constitution::storage_key::constitution_versions_prefix().len();
    malformed_version[hash_offset] ^= 1;
    assert_eq!(
        crate::constitution::storage_key::constitution_version_from_key(&malformed_version),
        None
    );

    let mut malformed_proof = crate::constitution::storage_key::constitution_guard_vote_proof(9);
    let proof_hash_offset = malformed_proof.len() - 16 - 4;
    malformed_proof[proof_hash_offset] ^= 1;
    assert_eq!(
        crate::constitution::storage_key::constitution_proof_version_from_key(&malformed_proof),
        None
    );
}

#[test]
fn full_check_required_for_legislation_or_runtime_code_delta() {
    let mut delta = BTreeMap::new();
    delta.insert(b"OtherPalletKey".to_vec(), Some(vec![1]));
    assert!(!needs_full_invariant_check(&delta));

    let mut legislation_key = crate::constitution::storage_key::pallet_prefix().to_vec();
    legislation_key.extend_from_slice(b"SomeStorage");
    delta.insert(legislation_key, Some(vec![2]));
    assert!(needs_full_invariant_check(&delta));

    let mut runtime_delta = BTreeMap::new();
    runtime_delta.insert(sp_storage::well_known_keys::CODE.to_vec(), Some(vec![3]));
    assert!(needs_full_invariant_check(&runtime_delta));
}

#[test]
fn existing_history_and_amendment_proofs_are_frozen_against_runtime_rewrites() {
    let version_key = crate::constitution::storage_key::law_version(CONSTITUTION_LAW_ID, 2);
    let referendum_key = crate::constitution::storage_key::constitution_amendment_proof(2);
    let guard_key = crate::constitution::storage_key::constitution_guard_vote_proof(2);
    let parent = BTreeMap::from([
        (version_key.clone(), vec![1, 2, 3]),
        (referendum_key.clone(), (100u64, 80u64, 5u64).encode()),
        (guard_key.clone(), 4u32.encode()),
    ]);
    let read_parent = |key: &[u8]| Ok(parent.get(key).cloned());

    // 改写或删除既有历史版本，即使新内容自身结构合法，也必须按篡改拒绝。
    assert!(check_frozen_constitution_records(
        &BTreeMap::from([(version_key.clone(), Some(vec![9, 9, 9]))]),
        read_parent,
    )
    .is_err());
    assert!(check_frozen_constitution_records(
        &BTreeMap::from([(version_key.clone(), None)]),
        read_parent,
    )
    .is_err());

    // 把既有凭据替换成另一组仍满足阈值的数据，或直接删除，同样属于历史篡改。
    assert!(check_frozen_constitution_records(
        &BTreeMap::from([(referendum_key.clone(), Some((100u64, 90u64, 0u64).encode()))]),
        read_parent,
    )
    .is_err());
    assert!(check_frozen_constitution_records(
        &BTreeMap::from([(guard_key.clone(), Some(7u32.encode()))]),
        read_parent,
    )
    .is_err());
    assert!(
        check_frozen_constitution_records(&BTreeMap::from([(guard_key, None)]), read_parent,)
            .is_err()
    );

    // 已存在的历史版本不允许事后补写凭据；合法新版本及其同块凭据仍可进入后置校验。
    let late_proof = crate::constitution::storage_key::constitution_guard_vote_proof(2);
    let mut parent_without_guard_proof = parent.clone();
    parent_without_guard_proof.remove(&late_proof);
    assert!(check_frozen_constitution_records(
        &BTreeMap::from([(late_proof, Some(4u32.encode()))]),
        |key| Ok(parent_without_guard_proof.get(key).cloned()),
    )
    .is_err());

    let new_version = crate::constitution::storage_key::law_version(CONSTITUTION_LAW_ID, 3);
    let new_proof = crate::constitution::storage_key::constitution_guard_vote_proof(3);
    assert_eq!(
        check_frozen_constitution_records(
            &BTreeMap::from([
                (new_version, Some(vec![4, 5, 6])),
                (new_proof, Some(4u32.encode())),
            ]),
            read_parent,
        ),
        Ok(())
    );

    // 无状态变化的同值 delta 不应污染后续合法导入。
    assert_eq!(
        check_frozen_constitution_records(
            &BTreeMap::from([(version_key, Some(vec![1, 2, 3]))]),
            read_parent,
        ),
        Ok(())
    );
}

#[test]
fn proof_version_cannot_hide_above_declared_latest_version() {
    let hidden = crate::constitution::storage_key::constitution_guard_vote_proof(9);
    assert_eq!(
        check_version_key_range([&hidden], 2),
        Err(crate::constitution::GuardError::VersionOutsideDeclaredRange(9))
    );
}

#[test]
fn imported_state_precheck_passes_valid_state() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    let entries = valid_current_state(2, amended_articles(immutable_intact));
    assert_eq!(
        check_imported_state_key_values(entries.iter(), &reference),
        Ok(())
    );
}

#[test]
fn imported_state_precheck_rejects_mutated_immutable_article() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    let articles = amended_articles(|n| {
        if n == 1 {
            "第一条被 warp 态篡改".to_string()
        } else {
            immutable_intact(n)
        }
    });
    let entries = valid_current_state(2, articles);
    assert_eq!(
        check_imported_state_key_values(entries.iter(), &reference),
        Err("ImmutableArticleMutated(1)".to_string())
    );
}

#[test]
fn imported_state_precheck_rejects_missing_constitution_keys() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    let entries: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    assert_eq!(
        check_imported_state_key_values(entries.iter(), &reference),
        Err("ConstitutionLawMissing".to_string())
    );
}

#[test]
fn imported_state_checks_main_constitution_independently_of_child_storage() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    let valid = valid_current_state(2, amended_articles(immutable_intact));
    let bad = valid_current_state(
        2,
        amended_articles(|n| {
            if n == 1 {
                "第一条被子状态掩盖".to_string()
            } else {
                immutable_intact(n)
            }
        }),
    );
    for (root, parents) in [
        (vec![1], vec![b":child_storage:default:test".to_vec()]),
        (vec![1], Vec::new()),
        (Vec::new(), vec![b":child_storage:default:test".to_vec()]),
    ] {
        for child_first in [false, true] {
            for (main, child, expected) in [
                (valid.clone(), bad.clone(), Ok(())),
                (
                    bad.clone(),
                    valid.clone(),
                    Err("ImmutableArticleMutated(1)".to_string()),
                ),
                (
                    Vec::new(),
                    valid.clone(),
                    Err("ConstitutionLawMissing".to_string()),
                ),
            ] {
                let mut state = sc_client_api::KeyValueStates::from([
                    (Vec::new(), (main, Vec::new())),
                    (root.clone(), (child, parents.clone())),
                ]);
                if child_first {
                    state.0.reverse();
                }
                let imported = ImportedState::<Block> {
                    block: Default::default(),
                    state,
                };
                assert_eq!(
                    check_imported_state_immutable(&imported, &reference),
                    expected
                );
            }
        }
    }
}

#[test]
fn reference_derives_all_immutable_articles() {
    let r = crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state()))
        .expect("应能派生");
    for &n in IMMUTABLE_CONSTITUTION_ARTICLES.iter() {
        assert!(r.articles.contains_key(&n), "基准缺条号 {n}");
    }
}

#[test]
fn passes_when_immutable_unchanged_even_if_mutable_changed() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    // 新版本:不可修改条款原样,只改了可变条 5(核心章),特别案 + 挂通过公投凭据,bump latest_version=2。
    let mut arts = amended_articles(immutable_intact);
    arts[IMMUTABLE_CONSTITUTION_ARTICLES.len()] = article_bytes(5, "可变条款已被合法修改");
    let mut state = valid_current_state(2, arts);
    state.push(amendment_proof_entry(2, 100, 80, 5));
    assert_eq!(check_immutable_articles(reader(state), &reference), Ok(()));
}

// ── 第十九条章→档位:核心章条款改动须记录为特别案 ──

#[test]
fn reference_derives_core_articles_excluding_immutable() {
    let r =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    // 创世含核心章非禁改第 5 条 → 应入核心基准;禁改条不得混入核心基准。
    assert!(r.core_articles.contains_key(&5), "核心章基准缺第 5 条");
    for &n in IMMUTABLE_CONSTITUTION_ARTICLES.iter() {
        assert!(
            !r.core_articles.contains_key(&n),
            "核心章基准不应含禁改条 {n}"
        );
    }
}

#[test]
fn rejects_core_clause_change_without_special() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    // 改核心章第 5 条但版本记录为重要案(Major=2)→ 违反第十九条,拒块。
    let mut arts = amended_articles(immutable_intact);
    arts[IMMUTABLE_CONSTITUTION_ARTICLES.len()] = article_bytes(5, "核心条被改但走重要案");
    let state = valid_current_state_vt(2, arts, 2 /* Major */);
    assert_eq!(
        check_immutable_articles(reader(state), &reference),
        Err(crate::constitution::GuardError::CoreClauseNotSpecial(5))
    );
}

#[test]
fn allows_core_clause_change_with_special() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    // 改核心章第 5 条 + 特别案 + 挂通过公投凭据 → 合法。
    let mut arts = amended_articles(immutable_intact);
    arts[IMMUTABLE_CONSTITUTION_ARTICLES.len()] = article_bytes(5, "核心条经特别案修改");
    let mut state = valid_current_state_vt(2, arts, LAW_VOTE_TYPE_SPECIAL);
    state.push(amendment_proof_entry(2, 100, 80, 5));
    assert_eq!(check_immutable_articles(reader(state), &reference), Ok(()));
}

#[test]
fn rejects_core_clause_change_without_referendum_proof() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    // 核心章第 5 条改动 + 特别案,但缺永久公投凭据 → 拒。
    let mut arts = amended_articles(immutable_intact);
    arts[IMMUTABLE_CONSTITUTION_ARTICLES.len()] = article_bytes(5, "核心条改但无公投凭据");
    let state = valid_current_state_vt(2, arts, LAW_VOTE_TYPE_SPECIAL); // 不挂 proof
    assert_eq!(
        check_immutable_articles(reader(state), &reference),
        Err(crate::constitution::GuardError::CoreClauseReferendumMissing(2))
    );
}

#[test]
fn rejects_core_clause_change_with_failing_referendum() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    // 核心章第 5 条改动 + 特别案 + 公投未过口径(参与 45% <70%)→ 拒。
    let mut arts = amended_articles(immutable_intact);
    arts[IMMUTABLE_CONSTITUTION_ARTICLES.len()] = article_bytes(5, "核心条改但公投未过");
    let mut state = valid_current_state_vt(2, arts, LAW_VOTE_TYPE_SPECIAL);
    state.push(amendment_proof_entry(2, 100, 40, 5));
    assert_eq!(
        check_immutable_articles(reader(state), &reference),
        Err(crate::constitution::GuardError::CoreClauseReferendumNotPassed(2))
    );
}

#[test]
fn allows_unchanged_core_clause_with_non_special() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    // 核心章第 5 条原样不动,版本记录为重要案 → 核心章未变,不约束档位 → 合法。
    let state = valid_current_state_vt(2, amended_articles(immutable_intact), 2 /* Major */);
    assert_eq!(check_immutable_articles(reader(state), &reference), Ok(()));
}

// ── 第21条:一切修宪须挂通过口径(4/7)的护宪大法官终审凭据 ──

#[test]
fn rejects_amendment_without_guard_proof() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    // v2 修宪(核心条款不变、免公投),但无护宪终审凭据 → 拒。
    let state = vec![
        (
            crate::constitution::storage_key::law(CONSTITUTION_LAW_ID),
            law_scale(2),
        ),
        (
            crate::constitution::storage_key::law_version(CONSTITUTION_LAW_ID, 1),
            law_version_scale(1, genesis_articles()),
        ),
        (
            crate::constitution::storage_key::law_version(CONSTITUTION_LAW_ID, 2),
            law_version_scale_vt(2, amended_articles(immutable_intact), 2 /* Major */),
        ),
        laws_by_scope_entry(vec![CONSTITUTION_LAW_ID]),
        manifest_entry(),
    ];
    assert_eq!(
        check_immutable_articles(reader(state), &reference),
        Err(crate::constitution::GuardError::GuardReviewMissing(2))
    );
}

#[test]
fn rejects_amendment_with_failing_guard_review() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    // v2 修宪 + 护宪终审仅 3/7 赞成(<4)→ 拒。
    let state = vec![
        (
            crate::constitution::storage_key::law(CONSTITUTION_LAW_ID),
            law_scale(2),
        ),
        (
            crate::constitution::storage_key::law_version(CONSTITUTION_LAW_ID, 1),
            law_version_scale(1, genesis_articles()),
        ),
        (
            crate::constitution::storage_key::law_version(CONSTITUTION_LAW_ID, 2),
            law_version_scale_vt(2, amended_articles(immutable_intact), 2 /* Major */),
        ),
        laws_by_scope_entry(vec![CONSTITUTION_LAW_ID]),
        manifest_entry(),
        guard_proof_entry(2, 3),
    ];
    assert_eq!(
        check_immutable_articles(reader(state), &reference),
        Err(crate::constitution::GuardError::GuardReviewNotPassed(2))
    );
}

#[test]
fn rejects_when_an_immutable_article_mutated() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    let arts = amended_articles(|n| {
        if n == 1 {
            "第一条被篡改".to_string()
        } else {
            immutable_intact(n)
        }
    });
    assert_eq!(
        check_immutable_articles(reader(valid_current_state(2, arts)), &reference),
        Err(crate::constitution::GuardError::ImmutableArticleMutated(1))
    );
}

#[test]
fn rejects_when_an_immutable_article_deleted() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    // 删掉第 17 条。
    let arts: Vec<MArticle> = IMMUTABLE_CONSTITUTION_ARTICLES
        .iter()
        .filter(|&&n| n != 17)
        .map(|&n| article_bytes(n, &immutable_intact(n)))
        .collect();
    let state = valid_current_state(2, arts);
    assert_eq!(
        check_immutable_articles(reader(state), &reference),
        Err(crate::constitution::GuardError::ImmutableArticleMissing(17))
    );
}

#[test]
fn rejects_when_constitution_storage_missing() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    assert_eq!(
        check_immutable_articles(reader(vec![]), &reference),
        Err(crate::constitution::GuardError::ConstitutionLawMissing)
    );
}

// ── H1 元数据 / houses / 唯一性不变式 ──

/// 在合法当前态基础上,替换某个 key 的值后跑校验。
fn check_with_override(version: u32, key: Vec<u8>, value: Vec<u8>) -> Result<(), GuardError> {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    let mut state = valid_current_state(version, amended_articles(immutable_intact));
    state.retain(|(k, _)| k != &key);
    state.push((key, value));
    check_immutable_articles(reader(state), &reference)
}

#[test]
fn rejects_when_tier_changed() {
    // tier 改为 National(1),其余合法。
    let mut law = Vec::new();
    CONSTITUTION_LAW_ID.encode_to(&mut law);
    1u8.encode_to(&mut law); // tier = National
    0u32.encode_to(&mut law);
    Vec::<Vec<u8>>::new().encode_to(&mut law);
    Some(2u32).encode_to(&mut law);
    2u32.encode_to(&mut law);
    Option::<u32>::None.encode_to(&mut law);
    1u8.encode_to(&mut law); // Effective
    assert_eq!(
        check_with_override(
            2,
            crate::constitution::storage_key::law(CONSTITUTION_LAW_ID),
            law
        ),
        Err(crate::constitution::GuardError::ConstitutionTierChanged)
    );
}

#[test]
fn rejects_when_repealed() {
    let law = law_scale_full(2, LAW_STATUS_REPEALED, Vec::new());
    assert_eq!(
        check_with_override(
            2,
            crate::constitution::storage_key::law(CONSTITUTION_LAW_ID),
            law
        ),
        Err(crate::constitution::GuardError::ConstitutionRepealed)
    );
}

#[test]
fn allows_pending_status_during_amendment() {
    // status=Pending(0) 不应被拒(合法修宪窗口)。
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    let state = valid_pending_state(amended_articles(immutable_intact));
    assert_eq!(check_immutable_articles(reader(state), &reference), Ok(()));
}

#[test]
fn rejects_when_houses_changed() {
    // houses 改为非空(与创世空 houses 不一致)。
    let law = law_scale_full(2, 1, vec![b"LN001-NLG0G-000000001-2026".to_vec()]);
    assert_eq!(
        check_with_override(
            2,
            crate::constitution::storage_key::law(CONSTITUTION_LAW_ID),
            law
        ),
        Err(crate::constitution::GuardError::ConstitutionHousesChanged)
    );
}

#[test]
fn rejects_when_constitution_not_unique() {
    // LawsByScope[宪法][0] 多出第二部宪法 law_id=1。
    assert_eq!(
        check_with_override(
            2,
            crate::constitution::storage_key::laws_by_scope_constitution(),
            vec![0u64, 1u64].encode()
        ),
        Err(crate::constitution::GuardError::ConstitutionNotUnique)
    );
}

#[test]
fn rejects_when_constitution_hidden_from_scope_list() {
    // LawsByScope[宪法][0] 被清空(law_id=0 被隐藏)。
    assert_eq!(
        check_with_override(
            2,
            crate::constitution::storage_key::laws_by_scope_constitution(),
            Vec::<u64>::new().encode()
        ),
        Err(crate::constitution::GuardError::ConstitutionNotUnique)
    );
}

#[test]
fn rejects_runtime_manifest_mutation() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    let mut state = valid_current_state(1, genesis_articles());
    let mut manifest =
        crate::constitution::MImmutableManifest::decode(&mut &manifest_entry().1[..]).unwrap();
    manifest.article_hashes[0] = [9u8; 32];
    replace_entry(
        &mut state,
        crate::constitution::storage_key::manifest(),
        manifest.encode(),
    );
    assert_eq!(
        check_immutable_articles(reader(state), &reference),
        Err(crate::constitution::GuardError::ConstitutionManifestChanged)
    );
}

#[test]
fn rejects_law_value_with_wrong_identity() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    let mut state = valid_current_state(1, genesis_articles());
    let mut law = law_scale(1);
    law[..8].copy_from_slice(&9u64.encode());
    replace_entry(
        &mut state,
        crate::constitution::storage_key::law(CONSTITUTION_LAW_ID),
        law,
    );
    assert_eq!(
        check_immutable_articles(reader(state), &reference),
        Err(crate::constitution::GuardError::ConstitutionLawIdChanged)
    );
}

#[test]
fn rejects_invalid_status_or_version_pointers() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    let mut invalid_status = valid_current_state(1, genesis_articles());
    replace_entry(
        &mut invalid_status,
        crate::constitution::storage_key::law(CONSTITUTION_LAW_ID),
        law_scale_with_versions(Some(1), 1, None, 9, Vec::new()),
    );
    assert_eq!(
        check_immutable_articles(reader(invalid_status), &reference),
        Err(crate::constitution::GuardError::ConstitutionVersionStateInvalid)
    );

    let mut invalid_pointers = valid_current_state(2, amended_articles(immutable_intact));
    replace_entry(
        &mut invalid_pointers,
        crate::constitution::storage_key::law(CONSTITUTION_LAW_ID),
        law_scale_with_versions(Some(1), 2, None, LAW_STATUS_EFFECTIVE, Vec::new()),
    );
    assert_eq!(
        check_immutable_articles(reader(invalid_pointers), &reference),
        Err(crate::constitution::GuardError::ConstitutionVersionStateInvalid)
    );
}

#[test]
fn rejects_version_identity_and_content_hash_mismatch() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    let key = crate::constitution::storage_key::law_version(CONSTITUTION_LAW_ID, 1);

    let mut wrong_identity = valid_current_state(1, genesis_articles());
    let mut head = crate::constitution::MLawVersionHead::decode(
        &mut &law_version_scale(1, genesis_articles())[..],
    )
    .expect("应能解码版本夹具");
    head.version = 7;
    replace_entry(&mut wrong_identity, key.clone(), head.encode());
    assert_eq!(
        check_immutable_articles(reader(wrong_identity), &reference),
        Err(crate::constitution::GuardError::VersionNumberChanged {
            expected: 1,
            found: 7,
        })
    );

    let mut wrong_hash = valid_current_state(1, genesis_articles());
    let mut head = crate::constitution::MLawVersionHead::decode(
        &mut &law_version_scale(1, genesis_articles())[..],
    )
    .expect("应能解码版本夹具");
    head.content_hash = [8u8; 32];
    replace_entry(&mut wrong_hash, key, head.encode());
    assert_eq!(
        check_immutable_articles(reader(wrong_hash), &reference),
        Err(crate::constitution::GuardError::VersionContentHashChanged(
            1
        ))
    );
}

#[test]
fn rejects_duplicate_article_number() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    let mut articles = genesis_articles();
    articles.push(article_bytes(1, "伪造的第二份第一条"));
    assert_eq!(
        check_immutable_articles(reader(valid_current_state(1, articles)), &reference),
        Err(crate::constitution::GuardError::DuplicateArticleNumber(1))
    );
}

#[test]
fn rejects_duplicate_chapter_number() {
    let head = version_head(vec![
        chapter_bytes(3, vec![section_bytes(1, vec![article_bytes(1, "a")])]),
        chapter_bytes(3, vec![section_bytes(1, vec![article_bytes(2, "b")])]),
    ]);
    assert_eq!(
        validate_version_identity_and_structure(&head, GENESIS_CONSTITUTION_VERSION),
        Err(crate::constitution::GuardError::DuplicateChapterNumber(3))
    );
}

#[test]
fn rejects_duplicate_section_number_within_chapter() {
    let head = version_head(vec![chapter_bytes(
        4,
        vec![
            section_bytes(8, vec![article_bytes(1, "a")]),
            section_bytes(8, vec![article_bytes(2, "b")]),
        ],
    )]);
    assert_eq!(
        validate_version_identity_and_structure(&head, GENESIS_CONSTITUTION_VERSION),
        Err(crate::constitution::GuardError::DuplicateSectionNumber {
            chapter: 4,
            section: 8,
        })
    );
}

#[test]
fn allows_same_section_number_in_different_chapters() {
    let head = version_head(vec![
        chapter_bytes(1, vec![section_bytes(6, vec![article_bytes(10, "a")])]),
        chapter_bytes(2, vec![section_bytes(6, vec![article_bytes(20, "b")])]),
    ]);
    assert_eq!(
        validate_version_identity_and_structure(&head, GENESIS_CONSTITUTION_VERSION),
        Ok(())
    );
}

#[test]
fn rejects_duplicate_article_number_across_chapters() {
    let head = version_head(vec![
        chapter_bytes(1, vec![section_bytes(1, vec![article_bytes(9, "a")])]),
        chapter_bytes(2, vec![section_bytes(1, vec![article_bytes(9, "b")])]),
    ]);
    assert_eq!(
        validate_version_identity_and_structure(&head, GENESIS_CONSTITUTION_VERSION),
        Err(crate::constitution::GuardError::DuplicateArticleNumber(9))
    );
}

#[test]
fn rejects_tampered_historical_constitution_version() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    let mut state = valid_current_state(2, amended_articles(immutable_intact));
    let mut historical = genesis_articles();
    historical[0] = article_bytes(1, "历史第一条被篡改");
    replace_entry(
        &mut state,
        crate::constitution::storage_key::law_version(CONSTITUTION_LAW_ID, 1),
        law_version_scale(1, historical),
    );
    let read = reader(state);
    assert_eq!(
        check_immutable_version(&read, &reference, 1),
        Err(crate::constitution::GuardError::ImmutableArticleMutated(1))
    );
}

#[test]
fn imported_state_rejects_hidden_version_above_latest() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    let mut state = valid_current_state(1, genesis_articles());
    state.push((
        crate::constitution::storage_key::law_version(CONSTITUTION_LAW_ID, 9),
        law_version_scale(9, genesis_articles()),
    ));
    assert_eq!(
        check_imported_state_key_values(state.iter(), &reference),
        Err("VersionOutsideDeclaredRange(9)".to_string())
    );
}

#[test]
fn imported_state_rejects_malformed_version_and_proof_hashers() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();

    let mut bad_version = valid_current_state(1, genesis_articles());
    let mut malformed_version =
        crate::constitution::storage_key::law_version(CONSTITUTION_LAW_ID, 9);
    let offset = crate::constitution::storage_key::constitution_versions_prefix().len();
    malformed_version[offset] ^= 1;
    bad_version.push((malformed_version, law_version_scale(9, genesis_articles())));
    assert_eq!(
        check_imported_state_key_values(bad_version.iter(), &reference),
        Err("StorageKeyMalformed".to_string())
    );

    let mut bad_proof = valid_current_state(1, genesis_articles());
    let mut malformed_proof = crate::constitution::storage_key::constitution_guard_vote_proof(9);
    let offset = malformed_proof.len() - 16 - 4;
    malformed_proof[offset] ^= 1;
    bad_proof.push((malformed_proof, 4u32.encode()));
    assert_eq!(
        check_imported_state_key_values(bad_proof.iter(), &reference),
        Err("StorageKeyMalformed".to_string())
    );
}

#[test]
fn real_runtime_genesis_satisfies_full_constitution_guard() {
    use sp_runtime::BuildStorage;
    let storage = runtime::RuntimeGenesisConfig::default()
        .build_storage()
        .expect("应能构建当前 runtime 创世状态");
    let top = storage.top;
    let read = |key: &[u8]| top.get(key).cloned();
    let reference = crate::constitution::ImmutableReference::from_raw_reader(read)
        .expect("应能派生真实创世基准");
    assert_eq!(check_immutable_articles(read, &reference), Ok(()));
    let law = decode_law_head(
        top.get(&crate::constitution::storage_key::law(CONSTITUTION_LAW_ID))
            .expect("真实创世应含 Law(0)"),
    )
    .expect("真实创世 Law(0) 应可解码");
    let version_keys: Vec<Vec<u8>> = top
        .keys()
        .filter(|key| {
            key.starts_with(&crate::constitution::storage_key::constitution_versions_prefix())
        })
        .cloned()
        .collect();
    let versions = declared_constitution_versions(version_keys.iter(), law.latest_version)
        .expect("真实创世版本集合应连续");
    for version in versions {
        assert_eq!(check_immutable_version(&read, &reference, version), Ok(()));
    }
}

#[test]
fn fresh_chain_spec_genesis_satisfies_full_constitution_guard() {
    use sp_runtime::BuildStorage;

    if runtime::WASM_BINARY.is_none() {
        eprintln!("跳过 fresh chainspec 护宪验收：当前测试构建未嵌入 WASM_BINARY");
        return;
    }
    let storage = node::chain::fresh_genesis_config()
        .expect("应能构建当前源码 fresh chainspec")
        .build_storage()
        .expect("fresh chainspec 应能物化创世状态");
    let top = storage.top;
    let read = |key: &[u8]| top.get(key).cloned();
    let law_bytes = top
        .get(&crate::constitution::storage_key::law(CONSTITUTION_LAW_ID))
        .expect("fresh chainspec 应含 Law(0)");
    decode_law_head(law_bytes).unwrap_or_else(|error| {
        panic!(
            "fresh chainspec Law(0) 解码失败: {error:?}; len={}; bytes={}",
            law_bytes.len(),
            hex::encode(law_bytes)
        )
    });
    let reference = crate::constitution::ImmutableReference::from_raw_reader(read)
        .expect("fresh chainspec 应能派生护宪基准");
    assert_eq!(check_immutable_articles(read, &reference), Ok(()));
}

#[test]
fn effective_version_uses_explicit_pointer() {
    // Effective 和 Pending 都只读显式 effective_version;新法尚无生效版时返回错误。
    assert_eq!(
        effective_version_of_law(&law_scale_full(3, 1, vec![])).unwrap(),
        3
    );
    assert_eq!(
        effective_version_of_law(&law_scale_full(3, 0, vec![])).unwrap(),
        2
    );
    assert!(
        effective_version_of_law(&law_scale_full(1, 0, vec![])).is_err(),
        "新法待生效且尚无 effective_version 时不能再推断"
    );
}

// ---- manifest 交叉校验 ----
fn manifest_scale(numbers: Vec<u32>, hashes: Vec<[u8; 32]>) -> Vec<u8> {
    let mut bytes = Vec::new();
    numbers.encode_to(&mut bytes);
    hashes.encode_to(&mut bytes);
    bytes
}

fn correct_manifest(reference: &ImmutableReference) -> Vec<u8> {
    let numbers: Vec<u32> = IMMUTABLE_CONSTITUTION_ARTICLES.to_vec();
    let hashes: Vec<[u8; 32]> = numbers
        .iter()
        .map(|n| sp_crypto_hashing::blake2_256(&reference.articles[n]))
        .collect();
    manifest_scale(numbers, hashes)
}

#[test]
fn manifest_passes_when_list_and_hashes_consistent() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    assert!(verify_manifest(&correct_manifest(&reference), &reference).is_ok());
}

#[test]
fn manifest_rejects_wrong_list() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    // 清单缺条 → 与二进制不一致。
    let bad = manifest_scale(vec![1, 2, 3], vec![[0u8; 32]; 3]);
    assert!(verify_manifest(&bad, &reference).is_err());
}

#[test]
fn manifest_rejects_tampered_hash() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    let numbers: Vec<u32> = IMMUTABLE_CONSTITUTION_ARTICLES.to_vec();
    let mut hashes: Vec<[u8; 32]> = numbers
        .iter()
        .map(|n| sp_crypto_hashing::blake2_256(&reference.articles[n]))
        .collect();
    hashes[0] = [9u8; 32]; // 谎报第一条摘要
    assert!(verify_manifest(&manifest_scale(numbers, hashes), &reference).is_err());
}

#[test]
fn full_constitution_values_reject_trailing_bytes() {
    let reference =
        crate::constitution::ImmutableReference::from_raw_reader(reader(genesis_state())).unwrap();
    let mut manifest = correct_manifest(&reference);
    manifest.push(0xff);
    assert!(verify_manifest(&manifest, &reference).is_err());

    let mut guard = 4u32.encode();
    guard.push(0xff);
    assert_eq!(
        check_guard_review_proof(
            &|key| (key == crate::constitution::storage_key::constitution_guard_vote_proof(2))
                .then(|| guard.clone()),
            2,
        ),
        Err(crate::constitution::GuardError::GuardReviewMissing(2))
    );

    let mut referendum = (100u64, 80u64, 20u64).encode();
    referendum.push(0xff);
    assert_eq!(
        check_core_referendum_proof(
            &|key| {
                (key == crate::constitution::storage_key::constitution_amendment_proof(2))
                    .then(|| referendum.clone())
            },
            2,
        ),
        Err(crate::constitution::GuardError::CoreClauseReferendumMissing(2))
    );
}

#[test]
fn render_rebuilds_expected_anchors() {
    let scale = law_version_scale(1, vec![article_bytes(1, "正文")]);
    let html = render_constitution_html(&scale, &[1], None).expect("应能重建");
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.trim_end().ends_with("</html>"));
    assert!(html.contains(
        "<span class=\"doc-version-cn\">v1</span><span class=\"doc-version-en\">v1</span>"
    ));
    assert!(html.contains("href=\"#article-1\""));
    assert!(html.contains("id=\"article-1\" class=\"block article-block\""));
    assert!(html.contains(
            "<span class=\"cn heading-cn\">第1条<span class=\"immutable-badge immutable-badge-cn\">不可修改条款</span></span>"
        ));
    assert!(html.contains(
            "<span class=\"en heading-en\">Article 1<span class=\"immutable-badge immutable-badge-en\">Immutable Clause</span></span>"
        ));
}

#[test]
fn render_uses_chain_law_version_label() {
    let scale = law_version_scale(1, vec![article_bytes(1, "正文")]);
    let label = law_version_label_scale("创世版", Some("Genesis Edition"));
    let html = render_constitution_html(&scale, &[], Some(&label)).expect("应能重建");
    assert!(html.contains(
            "<span class=\"doc-version-cn\">创世版</span><span class=\"doc-version-en\">Genesis Edition</span>"
        ));
}

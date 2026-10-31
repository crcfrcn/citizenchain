//! 法律完整提案与有界生效处理的FRAME基准。
use crate::types::LawAction;
use crate::{pallet::*, Call, Config, LawStatus, Pallet, Tier, VoteType};
use codec::Encode;
use frame_benchmarking::v2::*;
use frame_support::{
    dispatch::DispatchClass,
    traits::{Get, Hooks},
    BoundedVec,
};
use frame_system::RawOrigin;
use primitives::genesis::LawRegion;
use sp_runtime::sp_std::{vec, vec::Vec};

pub struct Authority<AccountId> {
    pub caller: AccountId,
    pub actor: Vec<u8>,
    pub role: Vec<u8>,
    pub houses: Vec<Vec<u8>>,
    pub executive: Vec<u8>,
    pub legislature: Vec<u8>,
}
pub trait BenchmarkSetup: Config {
    fn authority(action: u32) -> Authority<Self::AccountId>;
}
fn title<T: Config>() -> TitleOf<T> {
    vec![b't'; T::MaxTitleLen::get() as usize]
        .try_into()
        .unwrap()
}

// 同时覆盖最密集合法结构与长正文；总输入受实际Normal区块字节上限约束。
fn chapters<T: Config>(dense: bool, small: bool) -> ChaptersOf<T> {
    let max = *T::BlockLength::get().max.get(DispatchClass::Normal) as usize;
    let mut budget = if small {
        1024
    } else {
        max.min(<T as votebase::Config>::MaxProposalObjectLen::get() as usize)
            .saturating_sub(65536)
    };
    let mut chapters = Vec::new();
    let mut number = 1u32;
    'outer: for c in 0..T::MaxChaptersPerLaw::get() {
        let mut sections = Vec::new();
        for s in 0..T::MaxSectionsPerChapter::get() {
            let mut articles = Vec::new();
            for _ in 0..T::MaxArticlesPerSection::get() {
                let text: TextOf<T> = vec![
                    b'x';
                    if dense {
                        1
                    } else {
                        T::MaxTextLen::get() as usize
                    }
                ]
                .try_into()
                .unwrap();
                let clauses = if dense {
                    (0..T::MaxClausesPerArticle::get())
                        .map(|i| Clause::<T> {
                            number: i + 1,
                            text: text.clone(),
                            text_en: None,
                        })
                        .collect::<Vec<_>>()
                        .try_into()
                        .unwrap()
                } else {
                    BoundedVec::default()
                };
                let article = Article::<T> {
                    number,
                    title: b"A".to_vec().try_into().unwrap(),
                    title_en: None,
                    body: text,
                    body_en: None,
                    clauses,
                };
                let size = article.encoded_size();
                if size > budget {
                    break;
                }
                budget -= size;
                number += 1;
                articles.push(article);
            }
            if articles.is_empty() {
                break;
            }
            sections.push(Section::<T> {
                number: s + 1,
                title: b"S".to_vec().try_into().unwrap(),
                title_en: None,
                articles: articles.try_into().unwrap(),
            });
            if budget < 128 {
                break;
            }
        }
        if sections.is_empty() {
            break 'outer;
        }
        chapters.push(Chapter::<T> {
            number: c + 1,
            title: b"C".to_vec().try_into().unwrap(),
            title_en: None,
            sections: sections.try_into().unwrap(),
        });
        if budget < 128 {
            break;
        }
    }
    let chapters: ChaptersOf<T> = chapters.try_into().unwrap();
    assert!(!chapters.is_empty());
    assert!(chapters.encoded_size() + 65536 <= max || small);
    chapters
}
fn summary<T: Config>(a: &Authority<T::AccountId>, effective_at: u64) -> LawProposalSummary<T> {
    LawProposalSummary {
        action: LawAction::Enact,
        law_id: 0,
        tier: Tier::National,
        region: LawRegion::NATIONAL,
        houses: a
            .houses
            .clone()
            .into_iter()
            .map(|c| c.try_into().unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap(),
        actor_cid_number: a.actor.clone().try_into().unwrap(),
        proposer_role_code: a.role.clone().try_into().unwrap(),
        executive_cid_number: a.executive.clone().try_into().unwrap(),
        legislature_cid_number: Some(a.legislature.clone().try_into().unwrap()),
        vote_type: VoteType::Special,
        title: title::<T>(),
        title_en: None,
        content_hash: [0; 32],
        effective_at,
    }
}
fn write<T: Config>(a: &Authority<T::AccountId>, content: ChaptersOf<T>, future: bool) -> u64 {
    let id = NextLawId::<T>::get();
    Pallet::<T>::write_law_version(
        1,
        summary::<T>(a, if future { u64::MAX } else { 0 }),
        content,
        0,
    )
    .unwrap();
    id
}

#[benchmarks(where T:BenchmarkSetup)]
mod benchmarks {
    use super::*;
    #[benchmark]
    fn propose_enact_law() {
        let a = T::authority(entitybase::business_action::ACTION_ENACT_LAW);
        let s = summary::<T>(&a, 0);
        let text = chapters::<T>(true, false);
        #[extrinsic_call]
        _(
            RawOrigin::Signed(a.caller),
            s.tier,
            s.region,
            s.houses,
            s.actor_cid_number,
            s.proposer_role_code,
            s.executive_cid_number,
            s.legislature_cid_number,
            s.vote_type,
            s.title,
            s.title_en,
            text,
            0,
        );
    }
    #[benchmark]
    fn enact_text() {
        let a = T::authority(entitybase::business_action::ACTION_ENACT_LAW);
        let s = summary::<T>(&a, 0);
        let text = chapters::<T>(false, false);
        #[extrinsic_call]
        propose_enact_law(
            RawOrigin::Signed(a.caller),
            s.tier,
            s.region,
            s.houses,
            s.actor_cid_number,
            s.proposer_role_code,
            s.executive_cid_number,
            s.legislature_cid_number,
            s.vote_type,
            s.title,
            s.title_en,
            text,
            0,
        );
    }
    #[benchmark]
    fn propose_amend_law() {
        let a = T::authority(entitybase::business_action::ACTION_AMEND_LAW);
        let id = write::<T>(&a, chapters::<T>(true, true), false);
        let s = summary::<T>(&a, 0);
        let text = chapters::<T>(true, false);
        #[extrinsic_call]
        _(
            RawOrigin::Signed(a.caller),
            id,
            s.actor_cid_number,
            s.proposer_role_code,
            s.executive_cid_number,
            s.legislature_cid_number,
            s.vote_type,
            s.title,
            s.title_en,
            text,
            0,
        );
    }
    #[benchmark]
    fn amend_text() {
        let a = T::authority(entitybase::business_action::ACTION_AMEND_LAW);
        let id = write::<T>(&a, chapters::<T>(true, true), false);
        let s = summary::<T>(&a, 0);
        let text = chapters::<T>(false, false);
        #[extrinsic_call]
        propose_amend_law(
            RawOrigin::Signed(a.caller),
            id,
            s.actor_cid_number,
            s.proposer_role_code,
            s.executive_cid_number,
            s.legislature_cid_number,
            s.vote_type,
            s.title,
            s.title_en,
            text,
            0,
        );
    }
    #[benchmark]
    fn propose_repeal_law() {
        let a = T::authority(entitybase::business_action::ACTION_REPEAL_LAW);
        let id = write::<T>(&a, chapters::<T>(true, true), false);
        let s = summary::<T>(&a, 0);
        #[extrinsic_call]
        _(
            RawOrigin::Signed(a.caller),
            id,
            s.actor_cid_number,
            s.proposer_role_code,
            s.executive_cid_number,
            s.legislature_cid_number,
            s.vote_type,
        );
    }
    #[benchmark]
    fn on_initialize_base() {
        let a = T::authority(entitybase::business_action::ACTION_ENACT_LAW);
        for _ in 0..T::MaxPendingActivations::get() {
            write::<T>(&a, chapters::<T>(true, true), true);
        }
        #[block]
        {
            Pallet::<T>::on_initialize(1u32.into());
        }
    }
    #[benchmark]
    fn process_activation() {
        let a = T::authority(entitybase::business_action::ACTION_ENACT_LAW);
        let id = write::<T>(&a, chapters::<T>(true, false), true);
        #[block]
        {
            assert!(Pallet::<T>::activate_version_if_due(
                id,
                1,
                u64::MAX,
                u64::MAX
            ));
        }
        assert_eq!(Laws::<T>::get(id).unwrap().status, LawStatus::Effective);
    }
    #[benchmark]
    fn retain_activation() {
        let a = T::authority(entitybase::business_action::ACTION_ENACT_LAW);
        let id = write::<T>(&a, chapters::<T>(true, false), true);
        #[block]
        {
            assert!(!Pallet::<T>::activate_version_if_due(id, 1, u64::MAX, 0));
        }
    }
}

//! 立法与表决域(卡 20260630-onchina-legislation-console-framework,Phase 0 地基)。
//!
//! 承载立法机构管理员在 OnChina 的「发起提案 → 院内/两院表决 → 查看进度」与大屏只读展示。
//! 提案以「提案类型(ProposalCategory)」为可扩展维度:法律案(本轮)/任免案/预算案(预留)。
//! 本域只做「组织提案数据 + 扫码冷签 + 提交 extrinsic + 读链展示」,绝不计票/推进状态机(全归投票引擎)。
//!
//! 现有实现包括提案类型、机构候选、链调用准备、链读与展示。
//! 链交互(`onchina/legislation/laws/chain_*.rs`)、HTTP 入口和大屏聚合已接入；预算及人事仍按各自模块声明的状态开放。
include!("../platform/interface.rs");
#[path = "."]
mod legislation {

    /// 预算案（类>款>项>目）字段 schema；未来由预算业务模块接入代表机构表决。
    #[path = "budget/mod.rs"]
    pub(crate) mod budget;
    #[path = "proposals/category.rs"]
    pub(crate) mod category;
    /// 提案进度只读投影（Proposal + 代表/法律元数据 + tally）。
    #[path = "proposals/chain_read_proposal.rs"]
    pub(crate) mod chain_read_proposal;
    /// 大屏只读看板(Phase 3)——本节点机构名册 × 活跃提案 × 逐席投票聚合,免登录只读。
    #[path = "display/mod.rs"]
    pub(crate) mod display;
    /// 立法与表决 HTTP handler(/api/legislation/* 发起/表决/读法律/读提案)。
    #[path = "proposals/handler.rs"]
    pub(crate) mod handler;
    /// 法律案——章节条款提案 + 院内/两院表决 + 签署的链交互编码器(Phase 1)。
    #[path = "laws/mod.rs"]
    pub(crate) mod law;
    #[path = "proposals/model.rs"]
    pub(crate) mod model;
    /// 任免案（人事任免职书）字段 schema；未来由任免业务模块接入代表机构表决。
    #[path = "personnel/mod.rs"]
    pub(crate) mod personnel;

    #[path = "workspace/manifest.rs"]
    pub(crate) mod workspace;

    use crate::modules::model::{ModuleBusiness, ModuleDescription, ModuleRoute};
    use crate::*;
    pub(crate) const VERSION: &str = "1.0.0";
    #[derive(Default)]
    pub(crate) struct Module;
    use self::category::{legislation_role, LegislationRole};
    use crate::modules::capability::{CapabilitySet, EMPTY};
    fn legislation_capabilities(role: LegislationRole) -> CapabilitySet {
        let (can_propose_legislation, cast_representative_vote) = match role {
            LegislationRole::ProposerHouse => (true, true),
            LegislationRole::ReviewHouse => (false, true),
            LegislationRole::ProposerOnly => (true, false),
        };
        CapabilitySet {
            view_own_admins: true,
            can_view_legislation: true,
            can_propose_legislation,
            cast_representative_vote,
            ..EMPTY
        }
    }
    fn declaration(
        code: &str,
    ) -> Option<(
        CapabilitySet,
        crate::workspace::model::WorkspaceKind,
        Vec<crate::workspace::model::WorkspaceSection>,
    )> {
        let caps = legislation_capabilities(legislation_role(code)?);
        Some((
            caps,
            crate::workspace::model::WorkspaceKind::Legislation,
            legislation::workspace::legislation_sections(caps),
        ))
    }
    fn purposes() -> Vec<String> {
        vec![
            law::action::PURPOSE_LEGISLATION_PROPOSE.into(),
            law::action::PURPOSE_LEGISLATION_REPRESENTATIVE_VOTE.into(),
        ]
    }
    fn actions() -> Vec<String> {
        Vec::new()
    }

    #[async_trait::async_trait]
    impl ModuleBusiness for Module {
        fn describe(&self) -> ModuleDescription {
            modules::describe_module(
                "legislation",
                VERSION,
                "frontend/legislation_index.ts",
                &["LegislationView", "DisplayScreen"],
                self.routes(),
                purposes(),
                actions(),
                declaration,
            )
        }
        fn routes(&self) -> Vec<ModuleRoute> {
            vec![
                modules::model::ModuleRoute::new(
                    "admin",
                    "/api/legislation/proposable",
                    &["GET"],
                    get(legislation::handler::list_proposable),
                ),
                modules::model::ModuleRoute::new(
                    "admin",
                    "/api/legislation/propose",
                    &["POST"],
                    post(legislation::handler::propose_legislation),
                ),
                modules::model::ModuleRoute::new(
                    "admin",
                    "/api/legislation/representative-vote",
                    &["POST"],
                    post(legislation::handler::cast_representative_vote),
                ),
                modules::model::ModuleRoute::new(
                    "admin",
                    "/api/legislation/laws",
                    &["GET"],
                    get(legislation::handler::list_laws),
                ),
                modules::model::ModuleRoute::new(
                    "admin",
                    "/api/legislation/laws/mine",
                    &["GET"],
                    get(legislation::handler::list_my_laws),
                ),
                modules::model::ModuleRoute::new(
                    "admin",
                    "/api/legislation/laws/{law_id}",
                    &["GET"],
                    get(legislation::handler::law),
                ),
                modules::model::ModuleRoute::new(
                    "admin",
                    "/api/legislation/proposals/{proposal_id}",
                    &["GET"],
                    get(legislation::handler::get_proposal_state),
                ),
                modules::model::ModuleRoute::new(
                    "public",
                    "/api/public/legislation/display/board",
                    &["GET"],
                    get(legislation::display::handler::display_board),
                ),
            ]
        }
        async fn recheck_submit(
            &self,
            state: &AppState,
            ctx: &authentication::login::AdminAuthContext,
            session: &transactions::session::ChainSignSession,
        ) -> Result<(), axum::response::Response> {
            law::action::recheck_module_submit(state, ctx, session).await
        }
    }
}
fn local_module() -> Option<Arc<dyn modules::model::ModuleBusiness>> {
    Some(Arc::new(legislation::Module::default()))
}
fn main() {
    run_backend(local_module());
}

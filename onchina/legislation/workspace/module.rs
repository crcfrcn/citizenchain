//! 本包真实业务声明、HTTP路由和原业务回调。
use crate::*;
use onchina::modules::model::{ModuleBusiness, ModuleDescription, ModuleRoute};
pub(crate) const VERSION: &str = "1.0.0";
#[derive(Default)]
pub(crate) struct Module;
use self::category::{legislation_role, LegislationRole};
use onchina::modules::capability::{CapabilitySet, EMPTY};
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
    onchina::workspace::model::WorkspaceKind,
    Vec<onchina::workspace::model::WorkspaceSection>,
)> {
    let caps = legislation_capabilities(legislation_role(code)?);
    Some((
        caps,
        onchina::workspace::model::WorkspaceKind::Legislation,
        crate::workspace::legislation_sections(caps),
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
                get(crate::handler::list_proposable),
            ),
            modules::model::ModuleRoute::new(
                "admin",
                "/api/legislation/propose",
                &["POST"],
                post(crate::handler::propose_legislation),
            ),
            modules::model::ModuleRoute::new(
                "admin",
                "/api/legislation/representative-vote",
                &["POST"],
                post(crate::handler::cast_representative_vote),
            ),
            modules::model::ModuleRoute::new(
                "admin",
                "/api/legislation/laws",
                &["GET"],
                get(crate::handler::list_laws),
            ),
            modules::model::ModuleRoute::new(
                "admin",
                "/api/legislation/laws/mine",
                &["GET"],
                get(crate::handler::list_my_laws),
            ),
            modules::model::ModuleRoute::new(
                "admin",
                "/api/legislation/laws/{law_id}",
                &["GET"],
                get(crate::handler::law),
            ),
            modules::model::ModuleRoute::new(
                "admin",
                "/api/legislation/proposals/{proposal_id}",
                &["GET"],
                get(crate::handler::get_proposal_state),
            ),
            modules::model::ModuleRoute::new(
                "public",
                "/api/public/legislation/display/board",
                &["GET"],
                get(crate::display::handler::display_board),
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

#[cfg(test)]
#[path = "../tests/module.rs"]
mod tests;

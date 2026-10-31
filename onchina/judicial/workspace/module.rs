//! 本包真实业务声明、HTTP路由和原业务回调。
use crate::*;
use onchina::modules::model::{ModuleBusiness, ModuleDescription, ModuleRoute};
pub(crate) const VERSION: &str = "1.0.0";
#[derive(Default)]
pub(crate) struct Module;
fn declaration(
    code: &str,
) -> Option<(
    modules::capability::CapabilitySet,
    onchina::workspace::model::WorkspaceKind,
    Vec<onchina::workspace::model::WorkspaceSection>,
)> {
    if code != "NJD" {
        return None;
    }
    let caps = modules::capability::base_capabilities(code);
    Some((
        caps,
        onchina::workspace::model::WorkspaceKind::Judicial,
        crate::workspace::judicial_sections(caps),
    ))
}
fn purposes() -> Vec<String> {
    Vec::new()
}
fn actions() -> Vec<String> {
    Vec::new()
}

#[async_trait::async_trait]
impl ModuleBusiness for Module {
    fn describe(&self) -> ModuleDescription {
        let mut description = modules::describe_module(
            "judicial",
            VERSION,
            "frontend/judicial_JudicialWorkspace.tsx",
            &["JudicialWorkspace"],
            self.routes(),
            purposes(),
            actions(),
            declaration,
        );
        description.frontend_workspace = Some("JudicialWorkspace".into());
        description
    }
    fn routes(&self) -> Vec<ModuleRoute> {
        vec![]
    }
}

#[cfg(test)]
#[path = "../tests/module.rs"]
mod tests;

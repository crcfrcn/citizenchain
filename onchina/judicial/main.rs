//! 司法模块入口，仅组织当前已有的工作台声明；未开放的业务仍保持关闭。
include!("../platform/interface.rs");
#[path = "."]
mod judicial {
#[path = "workspace/manifest.rs"]
pub(crate) mod workspace;

    use crate::*;
    use crate::modules::model::{ModuleBusiness, ModuleDescription, ModuleRoute};
    pub(crate) const VERSION: &str = "1.0.0";
    #[derive(Default)]
    pub(crate) struct Module;
fn declaration(code: &str) -> Option<(modules::capability::CapabilitySet, crate::workspace::model::WorkspaceKind, Vec<crate::workspace::model::WorkspaceSection>)> {
    if code != "NJD" { return None; }
    let caps = modules::capability::base_capabilities(code);
    Some((caps, crate::workspace::model::WorkspaceKind::Judicial, judicial::workspace::judicial_sections(caps)))
}
fn purposes() -> Vec<String> { Vec::new() }
fn actions() -> Vec<String> { Vec::new() }

    #[async_trait::async_trait]
    impl ModuleBusiness for Module {
        fn describe(&self) -> ModuleDescription {
            let mut description = modules::describe_module(
                "judicial", VERSION, "frontend/judicial_JudicialWorkspace.tsx", &["JudicialWorkspace"],
                self.routes(), purposes(), actions(), declaration,
            );
            description.frontend_workspace = Some("JudicialWorkspace".into());
            description
        }
        fn routes(&self) -> Vec<ModuleRoute> {
            vec![

            ]
        }

    }

}
fn local_module() -> Option<Arc<dyn modules::model::ModuleBusiness>> { Some(Arc::new(judicial::Module::default())) }
fn main() { run_backend(local_module()); }

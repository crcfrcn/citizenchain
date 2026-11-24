//! 机构岗位是任职和权限的唯一组织方式，不保存独立人员或钱包名单。
use primitives::cid::code::InstitutionCode;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionRolePermissionInfo {
    pub module_tag: String,
    pub action_code: u32,
    pub operation: u8,
    pub operation_label: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionAssignmentInfo {
    pub person_cid: String,
    pub account_id: Option<String>,
    pub binding_revision: Option<String>,
    pub family_name: String,
    pub given_name: String,
    pub term_start: u32,
    pub term_end: u32,
    pub assignment_source: u8,
    pub assignment_source_label: String,
    pub assignment_source_ref: String,
    pub active: bool,
    pub effective: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionRoleInfo {
    pub role_code: String,
    pub role_name: String,
    pub term_required: bool,
    pub seats: u32,
    pub active: bool,
    pub permissions: Vec<InstitutionRolePermissionInfo>,
    pub assignments: Vec<InstitutionAssignmentInfo>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionRolesState {
    pub cid_number: String,
    pub institution_code: InstitutionCode,
    pub kind: u8,
    pub block_hash: String,
    pub roles: Vec<InstitutionRoleInfo>,
}
/// 本机选择的人员任职；真实绑定版本来自identity，不是机构钱包世代。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoleAccess {
    pub cid_number: String,
    pub role_code: String,
    pub person_cid: String,
    pub account_id: String,
    pub binding_revision: String,
    pub activated_at_ms: u64,
}
impl InstitutionRolesState {
    pub fn current_assignment(
        &self,
        person: &str,
        role: &str,
    ) -> Option<&InstitutionAssignmentInfo> {
        self.roles
            .iter()
            .find(|r| r.active && r.role_code == role)?
            .assignments
            .iter()
            .find(|a| {
                a.person_cid == person
                    && a.active
                    && a.effective
                    && a.account_id.is_some()
                    && a.binding_revision.is_some()
            })
    }
    pub fn permits(
        &self,
        person: &str,
        role: &str,
        module: &str,
        action: u32,
        operation: u8,
    ) -> bool {
        self.current_assignment(person, role).is_some()
            && self.roles.iter().any(|r| {
                r.role_code == role
                    && r.permissions.iter().any(|p| {
                        p.module_tag == module
                            && p.action_code == action
                            && p.operation == operation
                    })
            })
    }
    pub fn matches_access(&self, access: &RoleAccess) -> bool {
        self.cid_number == access.cid_number
            && self
                .current_assignment(&access.person_cid, &access.role_code)
                .is_some_and(|a| {
                    a.account_id.as_deref() == Some(&access.account_id)
                        && a.binding_revision.as_deref() == Some(&access.binding_revision)
                })
    }
}
#[cfg(test)]
#[path = "../tests/roles.rs"]
mod tests;

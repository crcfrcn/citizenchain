// 通用本机构管理员与机构治理 API；注册局专属名册 API 归注册局。

import type, { AdminAuth } from './authentication_types';
import type, { InstitutionDetail } from './subjects_api';
import { adminHeaders, request } from './helpers_http';

export type OwnInstitutionAdminRow = {
  account_id: string;
  family_name: string;
  given_name: string;
  role_code: string;
  role_name: string;
  term_required: boolean;
  term_start: number;
  term_end: number;
  assignment_source: number;
  assignment_source_label: string;
  assignment_source_ref: string;
  balance_fen?: string | null;
  is_self: boolean;
};

export type OwnInstitutionAdminListOutput = {
  institution_code: string;
  cid_short_name?: string | null;
  rows: OwnInstitutionAdminRow[];
};

export type InstitutionGovernanceAdminInput = {
  account_id: string;
  family_name: string;
  given_name: string;
};

export type InstitutionRolePermissionInput = {
  module_tag: string;
  action_code: number;
  operation: 'PROPOSE' | 'VOTE';
};

export type InstitutionGovernanceRoleMutationInput = {
  mutation: 'CREATE' | 'RENAME' | 'DELETE';
  role_code?: string;
  role_name?: string;
  term_required?: boolean;
  permissions?: InstitutionRolePermissionInput[];
  assignments?: InstitutionGovernanceAssignmentTargetInput[];
};

export type InstitutionGovernanceAssignmentTargetInput = {
  account_id: string;
  term_start: number;
  term_end: number;
};

export type InstitutionGovernanceAssignmentChangeInput = {
  role_code: string;
  assignments: InstitutionGovernanceAssignmentTargetInput[];
};

export type PrepareInstitutionGovernanceInput = {
  cid_number: string;
  proposer_role_code: string;
  admins?: InstitutionGovernanceAdminInput[];
  role_mutations?: InstitutionGovernanceRoleMutationInput[];
  assignment_changes?: InstitutionGovernanceAssignmentChangeInput[];
  legal_representative_cid_number?: string | null;
  clear_legal_representative?: boolean;
};

export type PrepareInstitutionChainOutput = {
  request_id: string;
  cid_number: string;
  chain_action: number;
  call_data_hex: string;
  sign_request: string;
  expires_at: number;
};

export async function listOwnInstitutionAdmins(auth: AdminAuth): Promise<OwnInstitutionAdminListOutput> {
  return request<OwnInstitutionAdminListOutput>('/api/admin/own-institution-admins', {
    method: 'GET',
    headers: adminHeaders(auth),
  });
}

export async function getOwnInstitution(auth: AdminAuth): Promise<InstitutionDetail> {
  return request<InstitutionDetail>('/api/admin/own-institution', {
    method: 'GET',
    headers: adminHeaders(auth),
  });
}

export async function prepareInstitutionGovernance(
  auth: AdminAuth,
  input: PrepareInstitutionGovernanceInput,
): Promise<PrepareInstitutionChainOutput> {
  return request<PrepareInstitutionChainOutput>('/api/admin/institution/governance/prepare', {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      ...adminHeaders(auth),
    },
    body: JSON.stringify(input),
  });
}
